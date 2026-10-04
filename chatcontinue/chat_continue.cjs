#!/usr/bin/env node

const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const CONFIG_FILE = "config";

function readConfig() {
  const configPath = path.join(__dirname, CONFIG_FILE);
  if (!fs.existsSync(configPath)) {
    throw new Error("缺少 chatcontinue/config，请重新在扩展里同步配置。");
  }

  const raw = fs.readFileSync(configPath, "utf8");
  const parsed = JSON.parse(raw);
  const goBinaryPath = typeof parsed.goBinaryPath === "string" ? parsed.goBinaryPath.trim() : "";
  if (!goBinaryPath) {
    throw new Error("chatcontinue/config 中缺少 goBinaryPath。");
  }

  const extraEnv = parsed && typeof parsed.env === "object" && parsed.env ? parsed.env : {};
  const bridgePort = typeof parsed.bridgePort === "number" ? parsed.bridgePort : 0;
  const bridgeSecret = typeof parsed.bridgeSecret === "string" ? parsed.bridgeSecret : "";
  return { goBinaryPath, env: extraEnv, bridgePort, bridgeSecret };
}

function parseArgs(argv) {
  let autoContinue = false;
  let instruction = "";
  const reasonParts = [];

  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "--auto-continue") {
      autoContinue = true;
      continue;
    }
    if (value === "--instruction") {
      instruction = argv[index + 1] || "";
      index += 1;
      continue;
    }
    reasonParts.push(value);
  }

  return {
    autoContinue,
    instruction,
    reason: reasonParts.join(" ").trim() || "continue"
  };
}

function normalizeAttachmentKind(attachment) {
  const rawKind = typeof attachment?.kind === "string" ? attachment.kind.trim().toLowerCase() : "";
  if (rawKind === "image" || rawKind === "directory") {
    return rawKind;
  }

  const rawPath = typeof attachment?.path === "string" ? attachment.path.trim().toLowerCase() : "";
  if (/\.(png|jpe?g|gif|webp|bmp|svg|ico|heic|heif|tiff?)$/.test(rawPath)) {
    return "image";
  }

  return "file";
}

function printAttachmentPaths(attachments) {
  const imagePaths = [];
  const filePaths = [];
  const seen = new Set();

  for (const attachment of Array.isArray(attachments) ? attachments : []) {
    const filePath = typeof attachment?.path === "string" ? attachment.path.trim() : "";
    if (!filePath || seen.has(filePath)) {
      continue;
    }

    seen.add(filePath);
    if (normalizeAttachmentKind(attachment) === "image") {
      imagePaths.push(filePath);
    } else {
      filePaths.push(filePath);
    }
  }

  if (imagePaths.length > 0) {
    console.log("Image paths:");
    for (const imagePath of imagePaths) {
      console.log(imagePath);
    }
  }

  if (filePaths.length > 0) {
    console.log("File paths:");
    for (const filePath of filePaths) {
      console.log(filePath);
    }
  }
}

function main() {
  const { goBinaryPath, env, bridgePort, bridgeSecret } = readConfig();
  const parsedArgs = parseArgs(process.argv.slice(2));
  const promptDir = __dirname;

  const commandArgs = [
    "prompt",
    "wait"
  ];

  // Prefer HTTP bridge mode when port+secret are available
  if (bridgePort > 0 && bridgeSecret) {
    commandArgs.push("--port", String(bridgePort));
    commandArgs.push("--secret", bridgeSecret);
  } else {
    commandArgs.push("--prompt-dir", promptDir);
  }

  commandArgs.push("--reason", parsedArgs.reason);

  if (parsedArgs.autoContinue) {
    commandArgs.push("--auto-continue");
    if (parsedArgs.instruction) {
      commandArgs.push("--instruction", parsedArgs.instruction);
    }
  }

  const stdout = execFileSync(goBinaryPath, commandArgs, {
    encoding: "utf8",
    env: {
      ...process.env,
      ...env
    }
  });

  const result = JSON.parse(String(stdout || "{}").trim() || "{}");

  if (result.replaced) {
    process.exit(0);
  }

  if (result.shouldContinue) {
    console.log("User chose to continue");
    if (typeof result.userInstruction === "string" && result.userInstruction.trim()) {
      console.log("User instruction: " + result.userInstruction.trim());
    }
    printAttachmentPaths(result.attachments);
    return;
  }

  console.log("User chose to end");
}

main();
