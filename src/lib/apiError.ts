export interface ApiErrorPayload {
  code: string;
  message: string;
}

/** 浏览器 mock 与规范化后的后端错误共用的错误类型。 */
export class ApiError extends Error {
  readonly code: string;

  constructor(payload: ApiErrorPayload) {
    super(payload.message);
    this.name = "ApiError";
    this.code = payload.code;
  }
}

// Tauri invoke 失败时以序列化后的 AppError({ code, message })拒绝，
// 不是 Error 实例；code 是跨栈稳定契约，业务分支只允许按 code 判断。
function isApiErrorPayload(value: unknown): value is ApiErrorPayload {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  return typeof candidate.code === "string" && typeof candidate.message === "string";
}

export function apiErrorCode(error: unknown): string | undefined {
  if (error instanceof ApiError) {
    return error.code;
  }
  if (isApiErrorPayload(error)) {
    return error.code;
  }
  return undefined;
}

export function apiErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (isApiErrorPayload(error)) {
    return error.message;
  }
  if (typeof error === "string" && error) {
    return error;
  }
  return fallback;
}
