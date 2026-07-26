const alwaysIgnoredProjectDirectories = new Set([".next"]);

const normalizePath = (path: string): string => path.replaceAll("\\", "/").replace(/\/$/, "");

export function isSameOrNestedPath(path: string, root: string): boolean {
  const normalizedPath = normalizePath(path);
  const normalizedRoot = normalizePath(root);
  return normalizedPath === normalizedRoot || normalizedPath.startsWith(`${normalizedRoot}/`);
}

export function isIgnoredScanPath(path: string, scanRoots: string[], ignoredDirectoryNames: string[]): boolean {
  const normalizedPath = normalizePath(path);
  const root = scanRoots
    .map(normalizePath)
    .filter((item) => isSameOrNestedPath(normalizedPath, item))
    .sort((left, right) => right.length - left.length)[0];
  if (!root || normalizedPath === root) return false;
  const ignored = new Set([...alwaysIgnoredProjectDirectories, ...ignoredDirectoryNames]);
  return normalizedPath
    .slice(root.length + 1)
    .split("/")
    .some((segment) => ignored.has(segment));
}

export function projectNameFromPath(path: string): string {
  return normalizePath(path).split("/").filter(Boolean).at(-1) ?? path;
}
