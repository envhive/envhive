// 复制 CLI 产物到 src-tauri/binaries/，供 tauri bundle externalBin 打包
// 在 app/ 目录下由 beforeBuildCommand 调用（tauri.conf.json 配置）
//
// 注意：tauri-build 在编译期要求 externalBin 文件名为
// `<name>-<target-triple>[.exe]`（如 envhive-cli-x86_64-pc-windows-msvc.exe），
// 打包时 Tauri 会自动重命名为不带 triple 的用户可读名。
import { cpSync, mkdirSync } from "node:fs";
import { execSync } from "node:child_process";

const isWin = process.platform === "win32";
const ext = isWin ? ".exe" : "";
const triple = execSync("rustc -vV").toString().match(/host: (\S+)/)[1];
const src = `src-tauri/target/release/envhive-cli${ext}`;
const dst = `src-tauri/binaries/envhive-cli-${triple}${ext}`;

mkdirSync("src-tauri/binaries", { recursive: true });
cpSync(src, dst);
console.log(`copied ${src} -> ${dst}`);
