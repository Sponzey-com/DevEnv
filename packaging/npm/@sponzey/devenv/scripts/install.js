#!/usr/bin/env node

const crypto = require("crypto");
const fs = require("fs");
const http = require("http");
const https = require("https");
const path = require("path");
const zlib = require("zlib");

const packageJson = require("../package.json");

const packageRoot = path.join(__dirname, "..");
const vendorDir = path.join(packageRoot, "vendor");
const binaryName = process.platform === "win32" ? "devenv.exe" : "devenv";
const binaryPath = path.join(vendorDir, binaryName);

function targetTriple() {
  const targets = {
    darwin: {
      arm64: "aarch64-apple-darwin",
      x64: "x86_64-apple-darwin",
    },
    linux: {
      arm64: "aarch64-unknown-linux-musl",
      x64: "x86_64-unknown-linux-musl",
    },
    win32: {
      x64: "x86_64-pc-windows-msvc",
    },
  };

  const byPlatform = targets[process.platform];
  const target = byPlatform ? byPlatform[process.arch] : undefined;
  if (!target) {
    throw new Error(`unsupported platform for @sponzey/devenv: ${process.platform}/${process.arch}`);
  }
  return target;
}

function artifactUrl(target) {
  const version = packageJson.version;
  const artifactName = `devenv-${version}-${target}.tar.gz`;
  const baseUrl = process.env.DEVENV_NPM_ARTIFACT_BASE_URL
    || `https://github.com/Sponzey-com/DevEnv/releases/download/v${version}`;
  return `${baseUrl.replace(/\/$/, "")}/${artifactName}`;
}

function download(url, redirects = 0) {
  if (redirects > 5) {
    return Promise.reject(new Error(`too many redirects while downloading ${url}`));
  }

  return new Promise((resolve, reject) => {
    const client = url.startsWith("https:") ? https : http;
    const request = client.get(url, {
      headers: {
        "user-agent": `@sponzey/devenv/${packageJson.version}`,
      },
    }, (response) => {
      const statusCode = response.statusCode || 0;
      const location = response.headers.location;

      if (statusCode >= 300 && statusCode < 400 && location) {
        response.resume();
        const nextUrl = new URL(location, url).toString();
        download(nextUrl, redirects + 1).then(resolve, reject);
        return;
      }

      if (statusCode !== 200) {
        response.resume();
        reject(new Error(`GET ${url} failed with HTTP ${statusCode}`));
        return;
      }

      const chunks = [];
      response.on("data", (chunk) => chunks.push(chunk));
      response.on("end", () => resolve(Buffer.concat(chunks)));
    });

    request.on("error", reject);
    request.setTimeout(120000, () => {
      request.destroy(new Error(`download timed out: ${url}`));
    });
  });
}

function readTarString(buffer, offset, length) {
  const end = offset + length;
  let firstNull = offset;
  while (firstNull < end && buffer[firstNull] !== 0) {
    firstNull += 1;
  }
  return buffer.toString("utf8", offset, firstNull);
}

function extractBinaryFromTarGz(archiveBuffer) {
  const tarBuffer = zlib.gunzipSync(archiveBuffer);
  let offset = 0;

  while (offset + 512 <= tarBuffer.length) {
    const name = readTarString(tarBuffer, offset, 100);
    if (!name) {
      break;
    }

    const prefix = readTarString(tarBuffer, offset + 345, 155);
    const fullName = prefix ? `${prefix}/${name}` : name;
    const sizeText = readTarString(tarBuffer, offset + 124, 12).trim();
    const size = parseInt(sizeText || "0", 8);
    const typeFlag = tarBuffer[offset + 156];
    const dataStart = offset + 512;
    const dataEnd = dataStart + size;

    if ((typeFlag === 0 || typeFlag === 48) && path.posix.basename(fullName) === binaryName) {
      return tarBuffer.subarray(dataStart, dataEnd);
    }

    offset = dataStart + Math.ceil(size / 512) * 512;
  }

  throw new Error(`release archive does not contain ${binaryName}`);
}

async function main() {
  if (process.env.DEVENV_NPM_SKIP_DOWNLOAD === "1") {
    console.log("Skipping DevEnv binary download because DEVENV_NPM_SKIP_DOWNLOAD=1");
    return;
  }

  const target = targetTriple();
  const url = artifactUrl(target);
  const checksumUrl = `${url}.sha256`;

  console.log(`Downloading DevEnv ${packageJson.version} for ${target}`);
  const [archiveBuffer, checksumBuffer] = await Promise.all([
    download(url),
    download(checksumUrl),
  ]);

  const expectedChecksum = checksumBuffer.toString("utf8").trim().split(/\s+/)[0];
  const actualChecksum = crypto.createHash("sha256").update(archiveBuffer).digest("hex");
  if (!expectedChecksum || expectedChecksum !== actualChecksum) {
    throw new Error(`checksum mismatch for ${url}`);
  }

  const binaryBuffer = extractBinaryFromTarGz(archiveBuffer);
  fs.mkdirSync(vendorDir, { recursive: true });

  const tmpPath = path.join(vendorDir, `${binaryName}.tmp-${process.pid}`);
  fs.writeFileSync(tmpPath, binaryBuffer, { mode: 0o755 });
  fs.renameSync(tmpPath, binaryPath);

  if (process.platform !== "win32") {
    fs.chmodSync(binaryPath, 0o755);
  }

  console.log(`Installed ${binaryPath}`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
