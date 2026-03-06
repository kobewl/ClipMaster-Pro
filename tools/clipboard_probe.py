import ctypes
import ctypes.wintypes as wt
import time


user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

user32.OpenClipboard.argtypes = [wt.HWND]
user32.OpenClipboard.restype = wt.BOOL
user32.CloseClipboard.argtypes = []
user32.CloseClipboard.restype = wt.BOOL
user32.EnumClipboardFormats.argtypes = [wt.UINT]
user32.EnumClipboardFormats.restype = wt.UINT
user32.IsClipboardFormatAvailable.argtypes = [wt.UINT]
user32.IsClipboardFormatAvailable.restype = wt.BOOL
user32.GetClipboardData.argtypes = [wt.UINT]
user32.GetClipboardData.restype = wt.HANDLE
user32.GetClipboardSequenceNumber.argtypes = []
user32.GetClipboardSequenceNumber.restype = wt.DWORD
user32.RegisterClipboardFormatW.argtypes = [wt.LPCWSTR]
user32.RegisterClipboardFormatW.restype = wt.UINT

kernel32.GlobalLock.argtypes = [wt.HGLOBAL]
kernel32.GlobalLock.restype = wt.LPVOID
kernel32.GlobalUnlock.argtypes = [wt.HGLOBAL]
kernel32.GlobalUnlock.restype = wt.BOOL

CF_UNICODETEXT = 13
CF_HDROP = 15
CF_HTML = user32.RegisterClipboardFormatW("HTML Format")


def get_text():
    if not user32.IsClipboardFormatAvailable(CF_UNICODETEXT):
        return None
    handle = user32.GetClipboardData(CF_UNICODETEXT)
    if not handle:
        return None
    ptr = kernel32.GlobalLock(handle)
    if not ptr:
        return None
    try:
        return ctypes.wstring_at(ptr)
    finally:
        kernel32.GlobalUnlock(handle)


def dump_clipboard():
    if not user32.OpenClipboard(None):
        print("open clipboard failed")
        return

    try:
        formats = []
        fmt = 0
        while True:
            fmt = user32.EnumClipboardFormats(fmt)
            if not fmt:
                break
            formats.append(fmt)

        text = get_text()
        preview = ""
        if text:
            preview = text[:120].replace("\r", "\\r").replace("\n", "\\n")

        print(f"formats={formats}")
        print(f"has_unicode={user32.IsClipboardFormatAvailable(CF_UNICODETEXT) != 0}")
        print(f"has_hdrop={user32.IsClipboardFormatAvailable(CF_HDROP) != 0}")
        print(f"has_html={user32.IsClipboardFormatAvailable(CF_HTML) != 0}")
        print(f"text={preview!r}")
    finally:
        user32.CloseClipboard()


def main():
    print("Watching clipboard for 30 seconds. Copy something in IDEA/PyCharm now.")
    last_seq = user32.GetClipboardSequenceNumber()
    print(f"initial_seq={last_seq}")
    deadline = time.time() + 30

    while time.time() < deadline:
        seq = user32.GetClipboardSequenceNumber()
        if seq != last_seq:
            print(f"\nsequence changed: {last_seq} -> {seq}")
            dump_clipboard()
            last_seq = seq
        time.sleep(0.1)

    print("done")


if __name__ == "__main__":
    main()
