#!/usr/bin/env node
import { spawn } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import readline from "node:readline/promises";

const args = parseArgs(process.argv.slice(2));
const url = requireArg(args, "url");
const timeoutMs = Number(args["timeout-ms"] ?? 90000);
const waitMs = Number(args["wait-ms"] ?? 1500);
const headed = Boolean(args.headed);
const manualChallenge = Boolean(args["manual-challenge"]);
const executable = args.executable ?? findChromeExecutable();
let tempProfile = null;
let chrome = null;

async function main() {
  try {
    const userDataDir = args["user-data-dir"] ?? await makeTempProfile();
    const wsUrl = await launchChrome(executable, userDataDir);
    const port = Number(new URL(wsUrl).port);
    const page = await createPage(port);
    const cdp = new CdpClient(page.webSocketDebuggerUrl);

    await cdp.connect();
    await cdp.send("Page.enable");
    await cdp.send("Runtime.enable");

    const loadPromise = cdp.waitFor("Page.loadEventFired", timeoutMs);
    await cdp.send("Page.navigate", { url });
    await loadPromise.catch(() => {});
    await delay(waitMs);

    let html = await getOuterHtml(cdp);
    if (manualChallenge && headed && looksLikeChallenge(html)) {
      const rl = readline.createInterface({ input: process.stdin, output: process.stderr });
      await rl.question("Browser challenge detected. Solve it in Chromium, then press Enter here...");
      rl.close();
      await delay(waitMs);
      html = await getOuterHtml(cdp);
    }

    if (manualChallenge && !headed && looksLikeChallenge(html)) {
      throw new Error("browser challenge detected; rerun with --chromium-headed --chromium-manual-challenge");
    }

    process.stdout.write(html);
    cdp.close();
  } finally {
    if (chrome) {
      chrome.kill("SIGTERM");
    }
    if (tempProfile) {
      await rm(tempProfile, { recursive: true, force: true }).catch(() => {});
    }
  }
}

function parseArgs(argv) {
  const parsed = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith("--")) {
      throw new Error(`unexpected argument: ${arg}`);
    }
    const key = arg.slice(2);
    const next = argv[i + 1];
    if (!next || next.startsWith("--")) {
      parsed[key] = true;
    } else {
      parsed[key] = next;
      i += 1;
    }
  }
  return parsed;
}

function requireArg(parsed, key) {
  const value = parsed[key];
  if (!value || value === true) {
    throw new Error(`missing required --${key}`);
  }
  return value;
}

async function makeTempProfile() {
  tempProfile = await mkdtemp(path.join(os.tmpdir(), "capitoltrades-chromium-"));
  return tempProfile;
}

function findChromeExecutable() {
  const platform = os.platform();
  const candidates = platform === "darwin"
    ? [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
      ]
    : platform === "win32"
      ? [
          "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
          "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
          "C:\\Program Files\\Chromium\\Application\\chrome.exe",
          "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
        ]
      : [
          "/usr/bin/google-chrome",
          "/usr/bin/google-chrome-stable",
          "/usr/bin/chromium",
          "/usr/bin/chromium-browser",
          "/snap/bin/chromium",
          "/usr/bin/microsoft-edge",
        ];

  return candidates.find((candidate) => candidate && existsSync(candidate));
}

async function launchChrome(chromePath, userDataDir) {
  if (!chromePath) {
    throw new Error("Chrome/Chromium executable not found; pass --chromium-executable");
  }

  const chromeArgs = [
    "--remote-debugging-port=0",
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-background-networking",
    "--disable-sync",
    `--user-data-dir=${userDataDir}`,
    "about:blank",
  ];
  if (!headed) {
    chromeArgs.unshift("--headless=new", "--disable-gpu");
  }

  chrome = spawn(chromePath, chromeArgs, { stdio: ["ignore", "ignore", "pipe"] });
  let stderrTail = "";
  chrome.on("exit", (code, signal) => {
    if (code !== null && code !== 0) {
      console.error(`Chromium exited with code ${code}`);
    } else if (signal) {
      console.error(`Chromium exited with signal ${signal}`);
    }
  });

  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("timed out waiting for DevTools endpoint")), timeoutMs);
    chrome.stderr.setEncoding("utf8");
    chrome.stderr.on("data", (chunk) => {
      stderrTail = `${stderrTail}${chunk}`.slice(-4000);
      const match = chunk.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match) {
        clearTimeout(timer);
        resolve(match[1]);
      }
    });
    chrome.on("error", (err) => {
      clearTimeout(timer);
      reject(err);
    });
    chrome.on("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(
        `Chromium exited before DevTools was ready: ${code ?? "signal"}${stderrTail ? `\n${stderrTail.trim()}` : ""}`,
      ));
    });
  });
}

async function createPage(port) {
  const targetUrl = `http://127.0.0.1:${port}/json/new?${encodeURIComponent("about:blank")}`;
  const response = await fetch(targetUrl, { method: "PUT" });
  if (!response.ok) {
    throw new Error(`failed to create Chrome target: HTTP ${response.status}`);
  }
  return response.json();
}

async function getOuterHtml(cdp) {
  const result = await cdp.send("Runtime.evaluate", {
    expression: "document.documentElement.outerHTML",
    returnByValue: true,
  });
  return result.result?.value ?? "";
}

function looksLikeChallenge(html) {
  const lower = html.toLowerCase();
  return [
    "captcha",
    "cf-challenge",
    "cloudflare",
    "checking your browser",
    "verify you are human",
    "just a moment",
  ].some((needle) => lower.includes(needle));
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

class CdpClient {
  constructor(wsUrl) {
    this.wsUrl = wsUrl;
    this.socket = null;
    this.nextId = 1;
    this.pending = new Map();
    this.eventWaiters = new Map();
    this.buffer = Buffer.alloc(0);
  }

  connect() {
    return new Promise((resolve, reject) => {
      const parsed = new URL(this.wsUrl);
      const key = randomBytes(16).toString("base64");
      const req = [
        `GET ${parsed.pathname}${parsed.search} HTTP/1.1`,
        `Host: ${parsed.host}`,
        "Upgrade: websocket",
        "Connection: Upgrade",
        `Sec-WebSocket-Key: ${key}`,
        "Sec-WebSocket-Version: 13",
        "\r\n",
      ].join("\r\n");

      this.socket = net.createConnection(Number(parsed.port), parsed.hostname, () => {
        this.socket.write(req);
      });

      let header = Buffer.alloc(0);
      const onHandshake = (chunk) => {
        header = Buffer.concat([header, chunk]);
        const end = header.indexOf("\r\n\r\n");
        if (end === -1) {
          return;
        }
        const rawHeader = header.slice(0, end).toString("utf8");
        if (!rawHeader.startsWith("HTTP/1.1 101")) {
          reject(new Error(`WebSocket upgrade failed: ${rawHeader.split("\r\n")[0]}`));
          return;
        }
        const accept = rawHeader.match(/Sec-WebSocket-Accept:\s*(.+)/i)?.[1]?.trim();
        const expected = createHash("sha1")
          .update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
          .digest("base64");
        if (accept !== expected) {
          reject(new Error("WebSocket accept header mismatch"));
          return;
        }
        this.socket.off("data", onHandshake);
        this.socket.on("data", (data) => this.onData(data));
        const remaining = header.slice(end + 4);
        if (remaining.length > 0) {
          this.onData(remaining);
        }
        resolve();
      };

      this.socket.on("data", onHandshake);
      this.socket.on("error", reject);
    });
  }

  send(method, params = {}) {
    const id = this.nextId;
    this.nextId += 1;
    this.writeFrame(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP command timed out: ${method}`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
    });
  }

  waitFor(method, ms) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        const waiters = this.eventWaiters.get(method) ?? [];
        this.eventWaiters.set(method, waiters.filter((waiter) => waiter.resolve !== resolve));
        reject(new Error(`timed out waiting for ${method}`));
      }, ms);
      const waiters = this.eventWaiters.get(method) ?? [];
      waiters.push({ resolve, timer });
      this.eventWaiters.set(method, waiters);
    });
  }

  onData(data) {
    this.buffer = Buffer.concat([this.buffer, data]);
    while (true) {
      const frame = readFrame(this.buffer);
      if (!frame) {
        return;
      }
      this.buffer = this.buffer.slice(frame.bytesRead);
      if (frame.opcode === 0x8) {
        this.close();
        return;
      }
      if (frame.opcode !== 0x1) {
        continue;
      }
      this.onMessage(frame.payload.toString("utf8"));
    }
  }

  onMessage(raw) {
    const message = JSON.parse(raw);
    if (message.id && this.pending.has(message.id)) {
      const pending = this.pending.get(message.id);
      this.pending.delete(message.id);
      clearTimeout(pending.timer);
      if (message.error) {
        pending.reject(new Error(message.error.message));
      } else {
        pending.resolve(message.result);
      }
      return;
    }

    if (message.method && this.eventWaiters.has(message.method)) {
      const waiters = this.eventWaiters.get(message.method);
      const waiter = waiters.shift();
      if (waiter) {
        clearTimeout(waiter.timer);
        waiter.resolve(message.params);
      }
      if (waiters.length === 0) {
        this.eventWaiters.delete(message.method);
      }
    }
  }

  writeFrame(text) {
    const payload = Buffer.from(text, "utf8");
    const mask = randomBytes(4);
    let headerLength = 6;
    if (payload.length >= 126 && payload.length < 65536) {
      headerLength = 8;
    } else if (payload.length >= 65536) {
      headerLength = 14;
    }
    const frame = Buffer.alloc(headerLength + payload.length);
    frame[0] = 0x81;
    if (payload.length < 126) {
      frame[1] = 0x80 | payload.length;
      mask.copy(frame, 2);
      maskPayload(payload).copy(frame, 6);
    } else if (payload.length < 65536) {
      frame[1] = 0x80 | 126;
      frame.writeUInt16BE(payload.length, 2);
      mask.copy(frame, 4);
      maskPayload(payload).copy(frame, 8);
    } else {
      frame[1] = 0x80 | 127;
      frame.writeBigUInt64BE(BigInt(payload.length), 2);
      mask.copy(frame, 10);
      maskPayload(payload).copy(frame, 14);
    }
    this.socket.write(frame);

    function maskPayload(source) {
      const out = Buffer.alloc(source.length);
      for (let i = 0; i < source.length; i += 1) {
        out[i] = source[i] ^ mask[i % 4];
      }
      return out;
    }
  }

  close() {
    this.socket?.destroy();
  }
}

function readFrame(buffer) {
  if (buffer.length < 2) {
    return null;
  }
  const opcode = buffer[0] & 0x0f;
  const masked = Boolean(buffer[1] & 0x80);
  let length = buffer[1] & 0x7f;
  let offset = 2;
  if (length === 126) {
    if (buffer.length < offset + 2) {
      return null;
    }
    length = buffer.readUInt16BE(offset);
    offset += 2;
  } else if (length === 127) {
    if (buffer.length < offset + 8) {
      return null;
    }
    length = Number(buffer.readBigUInt64BE(offset));
    offset += 8;
  }
  let mask = null;
  if (masked) {
    if (buffer.length < offset + 4) {
      return null;
    }
    mask = buffer.slice(offset, offset + 4);
    offset += 4;
  }
  if (buffer.length < offset + length) {
    return null;
  }
  const payload = Buffer.from(buffer.slice(offset, offset + length));
  if (mask) {
    for (let i = 0; i < payload.length; i += 1) {
      payload[i] ^= mask[i % 4];
    }
  }
  return { opcode, payload, bytesRead: offset + length };
}

await main();
