"""
AI Service - Multi-provider LLM integration.

For OpenAI / Custom providers:  works out-of-the-box via direct HTTP
(no extra dependencies).  LangChain is used when installed for richer
provider support (Anthropic, Google, Ollama, DeepSeek …).

Architecture is designed for two tiers (only free tier implemented now):
  - Free tier:  user provides their own API key / endpoint
  - Paid tier:  developer-provided API (future)
"""

import json
import threading
from typing import Tuple

from PyQt6.QtCore import QObject, pyqtSignal

from config.settings import Settings
from utils.logger import logger


# ── Helpers ──────────────────────────────────────────────────────────

def _langchain_available() -> bool:
    try:
        import langchain_core  # noqa: F401
        return True
    except ImportError:
        return False


# ── Lightweight message wrappers (used when LangChain is absent) ─────

class _Msg:
    """Minimal message object compatible with both DirectChat and LangChain."""
    def __init__(self, role: str, content: str):
        self.role = role
        self.content = content

class _SystemMsg(_Msg):
    def __init__(self, content: str):
        super().__init__("system", content)

class _HumanMsg(_Msg):
    def __init__(self, content: str):
        super().__init__("user", content)


# ── Direct OpenAI-compatible client (zero extra deps) ────────────────

class DirectOpenAIChat:
    """Lightweight OpenAI-compatible chat client using only urllib.
    Serves as the default backend for 'openai' and 'custom' providers
    so that AI features work without installing LangChain."""

    def __init__(self, *, model: str, api_key: str, base_url: str,
                 temperature: float = 0.3, max_tokens: int = 100):
        self.model = model
        self.api_key = api_key
        self.temperature = temperature
        self.max_tokens = max_tokens

        url = base_url.rstrip("/")
        if url.endswith("/chat/completions"):
            url = url.rsplit("/chat/completions", 1)[0]
        if "/v1" not in url:
            url += "/v1"
        self._endpoint = url + "/chat/completions"

    def invoke(self, messages) -> "_ChatResult":
        import urllib.request

        formatted = []
        for m in messages:
            if hasattr(m, "role") and hasattr(m, "content"):
                formatted.append({"role": m.role, "content": m.content})
            elif isinstance(m, dict):
                formatted.append(m)

        payload = {
            "model": self.model,
            "messages": formatted,
            "max_tokens": self.max_tokens,
            "temperature": self.temperature,
            "stream": False,
        }

        req = urllib.request.Request(
            self._endpoint,
            data=json.dumps(payload).encode("utf-8"),
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {self.api_key}",
            },
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            body = json.loads(resp.read().decode("utf-8"))
        return _ChatResult(body["choices"][0]["message"]["content"])


class _ChatResult:
    def __init__(self, content: str):
        self.content = content


# ── Provider Registry ────────────────────────────────────────────────

PROVIDERS = {
    "openai": {
        "display": "OpenAI",
        "package": "langchain-openai",
        "default_model": "gpt-3.5-turbo",
        "needs_key": True,
        "needs_url": False,
        "url_hint": "https://api.openai.com/v1 (optional override)",
        "direct_ok": True,   # works without LangChain
    },
    "anthropic": {
        "display": "Anthropic (Claude)",
        "package": "langchain-anthropic",
        "default_model": "claude-3-haiku-20240307",
        "needs_key": True,
        "needs_url": False,
        "url_hint": "",
        "direct_ok": False,
    },
    "google": {
        "display": "Google (Gemini)",
        "package": "langchain-google-genai",
        "default_model": "gemini-2.0-flash",
        "needs_key": True,
        "needs_url": False,
        "url_hint": "",
        "direct_ok": False,
    },
    "deepseek": {
        "display": "DeepSeek",
        "package": "langchain-deepseek",
        "default_model": "deepseek-chat",
        "needs_key": True,
        "needs_url": False,
        "url_hint": "",
        "direct_ok": True,   # OpenAI-compatible API
    },
    "ollama": {
        "display": "Ollama (Local)",
        "package": "langchain-ollama",
        "default_model": "llama3",
        "needs_key": False,
        "needs_url": True,
        "url_hint": "http://localhost:11434",
        "direct_ok": False,
    },
    "custom": {
        "display": "Custom (OpenAI-Compatible)",
        "package": "langchain-openai",
        "default_model": "gpt-3.5-turbo",
        "needs_key": True,
        "needs_url": True,
        "url_hint": "e.g. https://your-api.example.com/v1",
        "direct_ok": True,
    },
}

# Default base URLs for providers that support direct HTTP
_DEFAULT_URLS = {
    "openai": "https://api.openai.com/v1",
    "deepseek": "https://api.deepseek.com/v1",
}


def check_provider_available(provider: str) -> Tuple[bool, str]:
    """Return (ok, install_hint) for the given provider."""
    cfg = PROVIDERS.get(provider, {})

    # Providers with direct HTTP fallback always work
    if cfg.get("direct_ok"):
        return True, ""

    if not _langchain_available():
        return False, "pip install langchain langchain-core"

    try:
        if provider == "anthropic":
            import langchain_anthropic  # noqa: F401
        elif provider == "google":
            import langchain_google_genai  # noqa: F401
        elif provider == "ollama":
            import langchain_ollama  # noqa: F401
        return True, ""
    except ImportError:
        pkg = cfg.get("package", provider)
        return False, f"pip install {pkg}"


# ── LLM Factory ──────────────────────────────────────────────────────

def _create_llm(provider: str, model: str, api_key: str,
                api_url: str, temperature: float, max_tokens: int):
    """Create the best available chat client for the given provider."""

    # --- Providers that support direct HTTP fallback ---
    if provider in ("openai", "custom", "deepseek"):
        # Prefer LangChain if installed (richer features)
        if _langchain_available():
            try:
                from langchain_openai import ChatOpenAI
                kw = dict(model=model, api_key=api_key,
                          temperature=temperature, max_tokens=max_tokens)
                base = api_url or _DEFAULT_URLS.get(provider, "")
                if base:
                    kw["base_url"] = base
                return ChatOpenAI(**kw)
            except Exception as e:
                logger.info(f"LangChain ChatOpenAI failed ({e}); using direct HTTP")

        # Fallback: direct HTTP – always works
        base = api_url or _DEFAULT_URLS.get(provider, "")
        if not base:
            raise ValueError(f"API URL is required for provider '{provider}'")
        return DirectOpenAIChat(
            model=model, api_key=api_key, base_url=base,
            temperature=temperature, max_tokens=max_tokens,
        )

    # --- Providers that require LangChain ---
    if provider == "anthropic":
        from langchain_anthropic import ChatAnthropic
        return ChatAnthropic(
            model=model, api_key=api_key,
            temperature=temperature, max_tokens=max_tokens,
        )

    if provider == "google":
        from langchain_google_genai import ChatGoogleGenerativeAI
        return ChatGoogleGenerativeAI(
            model=model, google_api_key=api_key,
            temperature=temperature, max_output_tokens=max_tokens,
        )

    if provider == "ollama":
        from langchain_ollama import ChatOllama
        kw = dict(model=model, temperature=temperature,
                  num_predict=max_tokens)
        if api_url:
            kw["base_url"] = api_url
        return ChatOllama(**kw)

    raise ValueError(f"Unknown provider: {provider}")


# ── Prompt Builder ───────────────────────────────────────────────────

SYSTEM_PROMPT = (
    "You are a real-time text-prediction engine (like GitHub Copilot, but for general "
    "text). You run inside a smart clipboard manager.\n\n"
    "## Your Two Inputs\n"
    "1. **CLIPBOARD HISTORY** — the user's personal knowledge base: names, addresses, "
    "URLs, code snippets, templates, and frequently used phrases.\n"
    "2. **LIVE TYPING CONTEXT** — what the user is typing RIGHT NOW, character by "
    "character. THIS IS THE PRIMARY INPUT. The cursor is at the end of this text.\n\n"
    "## Your Task\n"
    "Predict the text the user will type NEXT — the characters that would come "
    "immediately after the cursor position. Your prediction should feel like a "
    "natural, seamless continuation of what the user is currently writing.\n\n"
    "## Strategy (in priority order)\n"
    "1. READ the live typing context carefully. Understand WHAT the user is doing "
    "(email? code? form? chat? document?) and WHERE in the sentence the cursor is.\n"
    "2. If the typing context PARTIALLY MATCHES a clipboard item, complete with "
    "the clipboard data (e.g. user typed 'Dear Mr. W' and clipboard has "
    "'Wang Liang' → predict 'ang Liang').\n"
    "3. If the user is composing free text, predict the most natural continuation "
    "based on the sentence so far AND any relevant clipboard data.\n"
    "4. If clipboard has structured data (address, phone, ID, URL) and the user "
    "seems to need it, suggest that data as the continuation.\n"
    "5. For code: predict the next statement, argument, or expression.\n\n"
    "## Output Rules\n"
    "- Output ONLY the continuation text — no quotes, no explanation, no prefix.\n"
    "- The continuation must start EXACTLY where the user's cursor is.\n"
    "- Predict a meaningful chunk (5-40 words), not just one word.\n"
    "- Do NOT repeat any text the user already typed.\n"
    "- If you truly cannot predict anything useful, output exactly: [NONE]"
)


def _build_messages(typing_context: str, clipboard_items: list) -> list:
    """Build prompt messages.  Works with or without LangChain."""
    try:
        from langchain_core.messages import SystemMessage, HumanMessage
        SysMsg, UsrMsg = SystemMessage, HumanMessage
    except ImportError:
        SysMsg, UsrMsg = _SystemMsg, _HumanMsg

    clip_lines = ""
    for i, item in enumerate(clipboard_items[:10], 1):
        preview = item.content[:300].replace("\n", " ")
        clip_lines += f"{i}. {preview}\n"

    # Extract the last few lines for emphasis — this is what the user
    # is actively writing RIGHT NOW
    trimmed = typing_context[-800:] if len(typing_context) > 800 else typing_context
    lines = trimmed.split("\n")
    current_line = lines[-1] if lines else trimmed
    recent_lines = "\n".join(lines[-5:]) if len(lines) > 5 else trimmed

    user_content = (
        f"## Clipboard Knowledge Base\n{clip_lines}\n"
        f"## Live Typing Context (most recent text the user has typed)\n"
        f"```\n{recent_lines}\n```\n\n"
        f"## Cursor Position\n"
        f"The cursor is at the END of this line:\n"
        f">>> {current_line}◂\n\n"
        f"Predict the text that comes IMMEDIATELY after the ◂ cursor:"
    )
    return [
        SysMsg(content=SYSTEM_PROMPT),
        UsrMsg(content=user_content),
    ]


# ── AI Service (Qt integration) ─────────────────────────────────────

class AIService(QObject):
    """Thread-safe LLM wrapper that emits Qt signals on completion."""

    prediction_ready = pyqtSignal(str)
    prediction_error = pyqtSignal(str)
    test_result = pyqtSignal(bool, str)   # (success, message)

    def __init__(self):
        super().__init__()
        self._llm = None
        self._gen = 0          # monotonic generation counter
        self._busy = False     # guard: at most 1 in-flight request
        self._reload()

    # ── Configuration ────────────────────────────────────────────────

    def _reload(self):
        ai = Settings.get("ai", {})
        self.provider = ai.get("provider", "openai")
        self.api_url = ai.get("api_url", "")
        self.api_key = ai.get("api_key", "")
        self.model = ai.get("model", "gpt-3.5-turbo")
        self.max_tokens = ai.get("max_tokens", 100)
        self.temperature = ai.get("temperature", 0.3)
        self.enabled = ai.get("enabled", False)

        self._llm = None
        if self.enabled:
            ok, hint = check_provider_available(self.provider)
            if not ok:
                logger.warning(f"Provider not available: {hint}")
                return
            try:
                self._llm = _create_llm(
                    self.provider, self.model, self.api_key,
                    self.api_url, self.temperature, self.max_tokens,
                )
                logger.info(f"LLM client created: provider={self.provider}, "
                            f"model={self.model}, backend={type(self._llm).__name__}")
            except Exception as e:
                logger.error(f"Failed to create LLM client: {e}")

    def reload_settings(self):
        self._reload()

    def is_configured(self) -> bool:
        return self._llm is not None and self.enabled

    # ── Prediction ───────────────────────────────────────────────────

    def cancel_pending(self):
        """Bump generation so in-flight results are discarded."""
        self._gen += 1

    def request_prediction(self, typing_context: str, clipboard_items: list,
                           gen: int = 0):
        if not self.is_configured():
            return
        if self._busy:
            return
        messages = _build_messages(typing_context, clipboard_items)
        req_gen = gen if gen else self._gen
        self._busy = True
        t = threading.Thread(
            target=self._invoke_gen,
            args=(messages, req_gen),
            daemon=True,
        )
        t.start()

    def test_connection(self):
        if not self._llm:
            self.test_result.emit(False,
                f"LLM client not created. Provider={self.provider}, "
                f"enabled={self.enabled}, llm={self._llm}")
            return
        try:
            from langchain_core.messages import HumanMessage
            messages = [HumanMessage(content="Reply with exactly: OK")]
        except ImportError:
            messages = [_HumanMsg(content="Reply with exactly: OK")]
        t = threading.Thread(
            target=self._invoke,
            args=(messages,
                  lambda _: self.test_result.emit(True, "Connection successful!"),
                  lambda e: self.test_result.emit(False, f"Failed: {e}")),
            daemon=True,
        )
        t.start()

    # ── Internal ─────────────────────────────────────────────────────

    def _invoke_gen(self, messages, req_gen: int):
        """Run LLM call; discard result if generation has advanced."""
        try:
            resp = self._llm.invoke(messages)
            content = resp.content.strip()
            if req_gen < self._gen:
                logger.debug("Discarding stale AI result (gen %d < %d)",
                             req_gen, self._gen)
                return
            self._on_result(content)
        except Exception as e:
            self._on_error(str(e))
        finally:
            self._busy = False

    def _invoke(self, messages, on_success, on_error):
        try:
            resp = self._llm.invoke(messages)
            on_success(resp.content.strip())
        except Exception as e:
            on_error(str(e))

    def _on_result(self, content: str):
        if content and not content.startswith("[NONE]"):
            cleaned = content.strip().strip('"').strip("'")
            if cleaned:
                self.prediction_ready.emit(cleaned)

    def _on_error(self, error: str):
        logger.warning(f"AI prediction request failed: {error}")
        self.prediction_error.emit(error)
