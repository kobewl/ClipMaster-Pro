"""测试 VK 映射修复"""
import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'src'))

from controllers.hotkey_controller import _parse_hotkey, _VK_MAP

# 测试反引号和其他特殊键
test_keys = [
    ("ctrl+`", 0x02, 0xC0),
    ("ctrl+~", 0x02, 0xC0),
    ("ctrl+shift+c", 0x06, 0x43),
    ("ctrl+f", 0x02, 0x46),
    ("ctrl+alt+o", 0x03, 0x4F),
    ("f1", 0x00, 0x70),
    ("ctrl+minus", 0x02, 0xBD),
    ("ctrl+-", 0x02, 0xBD),
    ("ctrl+equals", 0x02, 0xBB),
    ("ctrl+=", 0x02, 0xBB),
]

print("Testing VK mappings:")
print("=" * 50)
all_passed = True
for key_str, expected_mod, expected_vk in test_keys:
    modifiers, vk = _parse_hotkey(key_str)
    status = "OK" if (modifiers == expected_mod and vk == expected_vk) else "FAIL"
    if status == "FAIL":
        all_passed = False
    print(f"  {status}: '{key_str}' -> mod=0x{modifiers:02X} (exp 0x{expected_mod:02X}), vk=0x{vk:02X} (exp 0x{expected_vk:02X})")

print("=" * 50)
print("All tests passed!" if all_passed else "Some tests FAILED!")

# 显示所有可用的特殊字符
print("\nSpecial characters in VK_MAP:")
special_chars = [k for k in _VK_MAP.keys() if len(k) == 1 and not k.isalnum()]
print(f"  {special_chars}")
