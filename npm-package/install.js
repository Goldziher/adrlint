const crypto = require("node:crypto");
const fs = require("node:fs");
const https = require("node:https");
const os = require("node:os");
const path = require("node:path");
const tar = require("tar");
const AdmZip = require("adm-zip");

const { version } = require("./package.json");

const BASE_URL = `https://github.com/Goldziher/adrlint/releases/download/v${version}`;

// A release archive is a few megabytes. Refusing anything larger keeps a hostile or broken
// response from being buffered into memory.
const MAX_DOWNLOAD_BYTES = 100 * 1024 * 1024;

// Must stay in step with the archives .github/workflows/publish.yaml uploads.
const TRIPLES = {
  "Windows_NT:x64": "x86_64-pc-windows-msvc",
  "Windows_NT:arm64": "aarch64-pc-windows-msvc",
  "Linux:x64:glibc": "x86_64-unknown-linux-gnu",
  "Linux:arm64:glibc": "aarch64-unknown-linux-gnu",
  "Linux:x64:musl": "x86_64-unknown-linux-musl",
  "Linux:arm64:musl": "aarch64-unknown-linux-musl",
  "Darwin:x64": "x86_64-apple-darwin",
  "Darwin:arm64": "aarch64-apple-darwin",
};

function isMusl() {
  // Node reports the runtime libc in its diagnostic report: glibc sets glibcVersionRuntime, musl
  // leaves it unset. Older or minimal builds without a report fall back to Alpine's release file.
  const report = typeof process.report?.getReport === "function" ? process.report.getReport() : null;
  if (report && report.header) {
    return !report.header.glibcVersionRuntime;
  }
  return fs.existsSync("/etc/alpine-release");
}

function getPlatformTriple() {
  const type = os.type();
  const arch = os.arch();
  const libc = type === "Linux" ? (isMusl() ? "musl" : "glibc") : null;
  const key = libc ? `${type}:${arch}:${libc}` : `${type}:${arch}`;
  const triple = TRIPLES[key];

  if (!triple) {
    throw new Error(
      `No prebuilt adrlint binary for ${key}. Supported: ${Object.keys(TRIPLES).join(", ")}. Build from source with \`cargo install adrlint\`.`,
    );
  }

  return triple;
}

function httpsGet(url, maxRedirects = 5) {
  return new Promise((resolve, reject) => {
    if (maxRedirects <= 0) {
      return reject(new Error("Too many redirects"));
    }

    // HTTPS only. Following a redirect down to plain HTTP would let a network
    // attacker substitute the archive that gets unpacked below.
    if (new URL(url).protocol !== "https:") {
      return reject(new Error(`Refusing non-HTTPS download URL: ${url}`));
    }

    const req = https.get(url, { headers: { "User-Agent": "adrlint-npm-wrapper" } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        res.resume();
        return httpsGet(new URL(res.headers.location, url).toString(), maxRedirects - 1)
          .then(resolve)
          .catch(reject);
      }

      if (res.statusCode !== 200) {
        res.resume();
        return reject(new Error(`HTTP ${res.statusCode} ${res.statusMessage} for ${url}`));
      }

      const declared = Number(res.headers["content-length"]);
      if (Number.isFinite(declared) && declared > MAX_DOWNLOAD_BYTES) {
        res.resume();
        return reject(new Error(`Refusing to download ${url}: response declares ${declared} bytes`));
      }

      const chunks = [];
      let received = 0;
      res.on("data", (chunk) => {
        received += chunk.length;
        if (received > MAX_DOWNLOAD_BYTES) {
          chunks.length = 0;
          req.destroy(new Error(`Refusing to download ${url}: response exceeds ${MAX_DOWNLOAD_BYTES} bytes`));
          return;
        }
        chunks.push(chunk);
      });
      res.on("end", () => resolve(Buffer.concat(chunks)));
      res.on("error", reject);
    });

    req.on("error", reject);
    req.setTimeout(60000, () => {
      req.destroy(new Error(`Download timed out: ${url}`));
    });
  });
}

function sha256(buffer) {
  return crypto.createHash("sha256").update(buffer).digest("hex");
}

function expectedDigest(checksums, archiveName) {
  for (const line of checksums.toString("utf8").split("\n")) {
    const match = line.trim().match(/^([0-9a-f]{64})\s+\*?(\S+)$/);
    if (match && path.basename(match[2]) === archiveName) {
      return match[1];
    }
  }
  return null;
}

async function releaseChecksums() {
  return httpsGet(`${BASE_URL}/adrlint_${version}_checksums.txt`);
}

// A cached binary is only trusted if the release manifest still publishes the archive it was
// unpacked from and the binary itself still hashes to the digest recorded at install time. An
// `existsSync` check would happily hand a replaced or truncated file to the user.
async function cachedBinaryIsValid(binaryPath, recordPath, archiveName) {
  if (!fs.existsSync(binaryPath) || !fs.existsSync(recordPath)) {
    return false;
  }

  const publishedArchive = expectedDigest(await releaseChecksums(), archiveName);
  if (!publishedArchive) {
    return false;
  }

  const record = fs.readFileSync(recordPath);
  if (expectedDigest(record, archiveName) !== publishedArchive) {
    return false;
  }

  const recordedBinary = expectedDigest(record, path.basename(binaryPath));
  if (!recordedBinary) {
    return false;
  }

  return sha256(fs.readFileSync(binaryPath)) === recordedBinary;
}

async function installBinary() {
  const triple = getPlatformTriple();
  const isWindows = os.type() === "Windows_NT";
  const archiveName = `adrlint-${triple}.${isWindows ? "zip" : "tar.gz"}`;

  const binDir = path.join(__dirname, "bin");
  // bin/adrlint is the committed launcher stub; the native executable lives
  // beside it under a distinct name so extraction never overwrites the stub.
  const binaryName = isWindows ? "adrlint.exe" : "adrlint-bin";
  const binaryPath = path.join(binDir, binaryName);
  // Records `sha256(binary)  binary` and `sha256(archive)  archive` from the verified install, so
  // a later run can prove the cached binary is the one the release published.
  const recordPath = `${binaryPath}.sha256`;
  const memberName = isWindows ? "adrlint.exe" : "adrlint";

  fs.mkdirSync(binDir, { recursive: true });

  if (fs.existsSync(binaryPath)) {
    if (await cachedBinaryIsValid(binaryPath, recordPath, archiveName)) {
      return;
    }
    console.warn("adrlint: the cached binary does not match the release checksums; reinstalling.");
    fs.rmSync(binaryPath, { force: true });
    fs.rmSync(recordPath, { force: true });
  }

  const stagingDir = fs.mkdtempSync(path.join(os.tmpdir(), "adrlint-install-"));

  try {
    console.log(`Downloading ${archiveName}...`);
    const archive = await httpsGet(`${BASE_URL}/${archiveName}`);

    // Verify against the release's own checksum manifest before unpacking. The
    // archive readers below (node-tar, adm-zip) have a history of path-traversal
    // issues, so nothing hostile should reach them in the first place.
    const expected = expectedDigest(await releaseChecksums(), archiveName);
    if (!expected) {
      throw new Error(`No checksum published for ${archiveName}; refusing to install.`);
    }

    const actual = sha256(archive);
    if (actual !== expected) {
      throw new Error(`Checksum mismatch for ${archiveName}: expected ${expected}, got ${actual}`);
    }

    const archivePath = path.join(stagingDir, archiveName);
    fs.writeFileSync(archivePath, archive);

    console.log("Extracting...");
    if (isWindows) {
      const zip = new AdmZip(archivePath);
      const entry = zip.getEntries().find((e) => path.basename(e.entryName) === memberName);
      if (!entry) {
        throw new Error(`${memberName} not found in ${archiveName}`);
      }
      zip.extractEntryTo(entry, stagingDir, false, true);
    } else {
      await tar.extract({
        file: archivePath,
        cwd: stagingDir,
        filter: (entryPath) => path.basename(entryPath) === memberName,
      });
    }

    const extracted = path.join(stagingDir, memberName);
    if (!fs.existsSync(extracted)) {
      throw new Error(`${archiveName} did not contain ${memberName}`);
    }

    // copy + unlink rather than rename: the staging dir is in os.tmpdir(), which is
    // often a different filesystem from node_modules, and rename would fail EXDEV.
    // Removing the destination first means a pre-existing symlink there is replaced
    // instead of written through.
    fs.rmSync(binaryPath, { force: true });
    fs.copyFileSync(extracted, binaryPath);
    fs.chmodSync(binaryPath, 0o755);

    fs.rmSync(recordPath, { force: true });
    const record = `${sha256(fs.readFileSync(binaryPath))}  ${binaryName}\n${expected}  ${archiveName}\n`;
    fs.writeFileSync(recordPath, record, { flag: "wx", mode: 0o644 });

    console.log(`adrlint ${version} installed (${triple}).`);
  } finally {
    fs.rmSync(stagingDir, { recursive: true, force: true });
  }
}

installBinary().catch((error) => {
  console.error(`Failed to install the adrlint binary: ${error.message}`);
  process.exit(1);
});
