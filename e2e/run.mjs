import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const port = 4444;
const cargoDriver = resolve(process.env.HOME ?? "", ".cargo/bin/tauri-driver");
const driverPath = process.env.TAURI_DRIVER ?? (existsSync(cargoDriver) ? cargoDriver : "tauri-driver");
const application = process.env.TAURI_E2E_APP ?? resolve("src-tauri/target/debug/devpkg");
const artifactDirectory = resolve("output/playwright");
let sessionId;

const sleep = (duration) => new Promise((resolvePromise) => setTimeout(resolvePromise, duration));

async function request(path, options = {}) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    ...options,
    headers: { "content-type": "application/json", ...options.headers },
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(payload.value?.message ?? `WebDriver 请求失败：${response.status}`);
  }
  return payload.value;
}

async function waitForDriver(driver) {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    if (driver.driverError) throw driver.driverError;
    if (driver.exitCode !== null) throw new Error(`tauri-driver 异常退出：${driver.exitCode}`);
    try {
      await request("/status");
      return;
    } catch {
      await sleep(100);
    }
  }
  throw new Error("等待 tauri-driver 启动超时");
}

async function element(xpath) {
  const value = await request(`/session/${sessionId}/element`, {
    method: "POST",
    body: JSON.stringify({ using: "xpath", value: xpath }),
  });
  return value["element-6066-11e4-a52e-4f735466cecf"];
}

async function waitForElement(xpath) {
  let failure;
  for (let attempt = 0; attempt < 50; attempt += 1) {
    try {
      return await element(xpath);
    } catch (error) {
      failure = error;
      await sleep(100);
    }
  }
  throw failure;
}

async function click(xpath) {
  const id = await waitForElement(xpath);
  await request(`/session/${sessionId}/element/${id}/click`, { method: "POST", body: "{}" });
}

async function type(xpath, text) {
  const id = await waitForElement(xpath);
  await request(`/session/${sessionId}/element/${id}/clear`, { method: "POST", body: "{}" });
  await request(`/session/${sessionId}/element/${id}/value`, {
    method: "POST",
    body: JSON.stringify({ text, value: [...text] }),
  });
}

async function selectOption(selectXPath, value) {
  await click(`${selectXPath}/option[@value=${JSON.stringify(value)}]`);
}

async function screenshot() {
  if (!sessionId) return;
  try {
    const encoded = await request(`/session/${sessionId}/screenshot`);
    await writeFile(`${artifactDirectory}/failure.png`, Buffer.from(encoded, "base64"));
  } catch {
    // 测试失败时尽力保留截图，不覆盖原始异常。
  }
}

async function createSession() {
  const value = await request("/session", {
    method: "POST",
    body: JSON.stringify({
      capabilities: {
        alwaysMatch: {
          browserName: "wry",
          "tauri:options": { application },
        },
        firstMatch: [{}],
      },
    }),
  });
  sessionId = value.sessionId;
}

async function run() {
  if (process.platform === "darwin") {
    throw new Error(
      "tauri-driver 当前不支持 macOS；原生 E2E 请在 Linux CI 运行，macOS 使用 pnpm check 和 pnpm build:desktop 验证。",
    );
  }
  await mkdir(artifactDirectory, { recursive: true });
  const driverLog = [];
  const driver = spawn(driverPath, ["--port", String(port)], {
    env: { ...process.env, EASY_PACKAGE_E2E: "1" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  driver.on("error", (error) => {
    driver.driverError = error;
  });
  driver.stdout.on("data", (chunk) => driverLog.push(chunk.toString()));
  driver.stderr.on("data", (chunk) => driverLog.push(chunk.toString()));

  try {
    await waitForDriver(driver);
    await createSession();

    await waitForElement("//h1[normalize-space()='概览']");
    await waitForElement("//span[normalize-space()='Homebrew']");

    await click("//nav[@aria-label='主要导航']//button[.//span[normalize-space()='软件包']]");
    await selectOption("//label[.//span[normalize-space()='管理器']]//select", "npm");
    await waitForElement("//strong[normalize-space()='typescript']");
    await type("//input[@placeholder='搜索软件包']", "not-a-real-package");
    await waitForElement("//strong[normalize-space()='没有匹配的软件包']");

    await click("//nav[@aria-label='主要导航']//button[.//span[normalize-space()='环境']]");
    await waitForElement("//h1[normalize-space()='环境']");
    await waitForElement("//h2[normalize-space()='命令解析']");

    await click("//nav[@aria-label='主要导航']//button[.//span[normalize-space()='项目']]");
    await click("//nav[@aria-label='项目工作区导航']//button[normalize-space()='项目分析']");
    await selectOption("//select[@aria-label='依赖生态']", "JavaScript");
    await waitForElement("//h2[normalize-space()='react']");
    await waitForElement("//span[normalize-space()='pnpm-lock.yaml']");

    await click("//nav[@aria-label='主要导航']//button[.//span[normalize-space()='环境']]");
    await click("//nav[@aria-label='本机工作区导航']//button[normalize-space()='日志']");
    await waitForElement("//h2[normalize-space()='扫描记录']");
    await waitForElement("//*[normalize-space()='E2E 固定环境扫描完成']");

    await click("//nav[@aria-label='主要导航']//button[.//span[normalize-space()='概览']]");
    await click("//button[normalize-space()='刷新']");
    await click("//button[normalize-space()='取消扫描']");
    await waitForElement("//*[normalize-space()='本次扫描已取消，保留上次成功结果。']");
  } catch (error) {
    await screenshot();
    throw error;
  } finally {
    await writeFile(`${artifactDirectory}/tauri-driver.log`, driverLog.join(""));
    if (sessionId) await request(`/session/${sessionId}`, { method: "DELETE", body: "{}" }).catch(() => undefined);
    driver.kill("SIGTERM");
  }
}

run().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
