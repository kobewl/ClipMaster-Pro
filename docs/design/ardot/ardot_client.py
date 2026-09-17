#!/usr/bin/env python3
"""
Ardot MCP 客户端 —— 让命令行/脚本能直接调用 Ardot 的 26 个设计工具。

背景：Ardot MCP 是 HTTP + OAuth 的远程服务（https://ardot.tencent.com/mcp）。
ZCode 自己会用它，但脚本里要调用时，需要一个独立的轻量客户端。这里从
ZCode 的凭据库（~/.zcode/v2/credentials.json）里解出访问令牌再直接发 JSON-RPC。

凭据解密方式与 ZCode 一致：AES-256-GCM，密钥 = sha256(secret)，
secret 取环境变量 ZCODE_CREDENTIAL_SECRET，缺省回退到
`zcode-credential-fallback:{platform}:{home}:{username}`。

用法：
    python3 ardot_client.py call <tool> '<json_args>'     # 调用一个工具
    python3 ardot_client.py tools                         # 列出全部工具
    python3 ardot_client.py upload <本地文件> <mime类型>   # 上传资产，返回 downloadUrl
"""

import base64
import hashlib
import json
import os
import platform
import getpass
import sys
import urllib.request

MCP_URL = "https://ardot.tencent.com/mcp"
CREDS = os.path.expanduser("~/.zcode/v2/credentials.json")

# Ardot 这条凭据在 ZCode 凭据库里的键前缀。指向 ardot.tencent.com/mcp 的那条。
ARDOT_KEY_PREFIX = "mcp:oauth:fced3c4ccdb52dd0b61b539c"


# ---------------------------------------------------------------- 凭据解密

def _b64url(s: str) -> bytes:
    return base64.urlsafe_b64decode(s + "=" * (-len(s) % 4))


def _cipher_key() -> bytes:
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM  # noqa: F401
    secret = os.environ.get("ZCODE_CREDENTIAL_SECRET")
    if not secret:
        secret = (
            f"zcode-credential-fallback:{platform.system().lower()}:"
            f"{os.path.expanduser('~')}:{getpass.getuser()}"
        )
    return hashlib.sha256(secret.encode()).digest()


def _decrypt(value: str) -> str:
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM
    if not value.startswith("enc:v1:"):
        return value
    iv, tag, ct = value[7:].split(".")
    return AESGCM(_cipher_key()).decrypt(
        _b64url(iv), _b64url(ct) + _b64url(tag), None
    ).decode("utf-8")


def access_token() -> str:
    """取出 Ardot 的 OAuth 访问令牌。"""
    creds = json.load(open(CREDS, encoding="utf-8"))
    key = f"{ARDOT_KEY_PREFIX}:tokens"
    if key not in creds:
        raise SystemExit(
            "凭据库里没有 Ardot 令牌。请先在 ZCode 的 设置 → MCP 里完成 ardot-remote 授权。"
        )
    return json.loads(_decrypt(creds[key]))["access_token"]


# ---------------------------------------------------------------- JSON-RPC

def rpc(method: str, params: dict, timeout: int = 180) -> list:
    """发一次 JSON-RPC，返回解析后的消息列表（服务端用 SSE 回包）。"""
    body = json.dumps(
        {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
    ).encode()
    req = urllib.request.Request(
        MCP_URL,
        data=body,
        method="POST",
        headers={
            "Authorization": f"Bearer {access_token()}",
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream",
        },
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        raw = resp.read().decode()

    msgs = []
    for line in raw.splitlines():
        if line.startswith("data: "):
            msgs.append(json.loads(line[6:]))
    return msgs


def call(tool: str, args: dict | None = None):
    """调用一个 MCP 工具，返回解析后的内容（dict / str / 原始消息）。"""
    msgs = rpc("tools/call", {"name": tool, "arguments": args or {}})
    texts = []
    for msg in msgs:
        if "error" in msg:
            return f"ERROR: {json.dumps(msg['error'], ensure_ascii=False)}"
        for c in msg.get("result", {}).get("content", []):
            if c.get("type") == "text":
                texts.append(c["text"])
    joined = "\n".join(texts)
    try:
        return json.loads(joined)
    except (ValueError, TypeError):
        return joined


# ---------------------------------------------------------------- 资产上传

def register_asset(file_url: str, content_type: str) -> dict:
    """申请一对临时的上传/下载地址。"""
    r = call("register_assets", {"fileUrl": file_url, "contentType": content_type})
    if not isinstance(r, dict) or not r.get("success"):
        raise SystemExit(f"申请上传地址失败: {r}")
    return r["data"]


def upload_file(file_url: str, path: str, content_type: str) -> str:
    """把本地文件传到 Ardot 的临时存储，返回下载地址（可直接给其它工具用）。"""
    slot = register_asset(file_url, content_type)
    data = open(path, "rb").read()
    req = urllib.request.Request(
        slot["uploadUrl"],
        data=data,
        method="PUT",
        headers={"Content-Type": content_type},
    )
    with urllib.request.urlopen(req, timeout=180) as resp:
        if resp.status not in (200, 201, 204):
            raise SystemExit(f"上传失败，HTTP {resp.status}")
    return slot["downloadUrl"]


# ---------------------------------------------------------------- CLI

def main(argv: list) -> int:
    if len(argv) < 2:
        print(__doc__)
        return 1

    cmd = argv[1]

    if cmd == "tools":
        msgs = rpc("tools/list", {})
        tools = msgs[0]["result"]["tools"]
        for t in tools:
            print(f"{t['name']:28} {(t.get('description') or '').splitlines()[0][:80]}")
        return 0

    if cmd == "call":
        tool = argv[2]
        args = json.loads(argv[3]) if len(argv) > 3 else {}
        r = call(tool, args)
        print(json.dumps(r, ensure_ascii=False, indent=2) if not isinstance(r, str) else r)
        return 0

    if cmd == "upload":
        path, mime = argv[2], argv[3]
        file_url = argv[4] if len(argv) > 4 else None
        if not file_url:
            raise SystemExit("需要 fileUrl，例如 https://ardot.tencent.com/file/<id>")
        print(upload_file(file_url, path, mime))
        return 0

    print(f"未知命令: {cmd}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
