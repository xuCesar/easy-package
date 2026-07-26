import { describe, expect, it } from "vitest";
import { ApiError, apiErrorCode, apiErrorMessage } from "./apiError";

describe("apiError", () => {
  it("从 Tauri 结构化 payload 提取 code 与 message", () => {
    const rejection: unknown = { code: "SCAN_CANCELLED", message: "扫描已取消" };
    expect(apiErrorCode(rejection)).toBe("SCAN_CANCELLED");
    expect(apiErrorMessage(rejection, "fallback")).toBe("扫描已取消");
  });

  it("支持 ApiError 实例", () => {
    const error = new ApiError({ code: "PACKAGE_ACTION_RECOVERY_REQUIRED", message: "请先扫描" });
    expect(error).toBeInstanceOf(Error);
    expect(apiErrorCode(error)).toBe("PACKAGE_ACTION_RECOVERY_REQUIRED");
    expect(apiErrorMessage(error, "fallback")).toBe("请先扫描");
  });

  it("普通 Error、字符串与未知值分别取 message、原文与兜底文案", () => {
    expect(apiErrorCode(new Error("boom"))).toBeUndefined();
    expect(apiErrorMessage(new Error("boom"), "fallback")).toBe("boom");
    expect(apiErrorMessage("直接字符串", "fallback")).toBe("直接字符串");
    expect(apiErrorMessage(42, "fallback")).toBe("fallback");
    expect(apiErrorMessage({ code: 1, message: "x" }, "fallback")).toBe("fallback");
    expect(apiErrorCode({ code: 1, message: "x" })).toBeUndefined();
  });
});
