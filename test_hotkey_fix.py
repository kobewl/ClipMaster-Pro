"""测试热键修复是否正常工作"""
import sys
import os

# 添加 src 到路径
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'src'))

from PyQt6.QtWidgets import QApplication, QLabel, QVBoxLayout, QWidget
from PyQt6.QtCore import Qt, QTimer
from controllers.hotkey_controller import HotkeyController, _parse_hotkey

def test_parse_hotkey():
    """测试热键解析函数"""
    print("=" * 50)
    print("测试热键解析...")

    test_cases = [
        ("Ctrl+O", 0x0002, 0x4F),      # Ctrl + O
        ("Ctrl+Shift+C", 0x0006, 0x43), # Ctrl + Shift + C
        ("Ctrl+F", 0x0002, 0x46),      # Ctrl + F
        ("Alt+Tab", 0x0001, 0x09),     # Alt + Tab
        ("Ctrl+Win+D", 0x000A, 0x44),  # Ctrl + Win + D
    ]

    all_passed = True
    for key_str, expected_mod, expected_vk in test_cases:
        modifiers, vk = _parse_hotkey(key_str)
        if modifiers == expected_mod and vk == expected_vk:
            print(f"  ✓ {key_str} -> modifiers=0x{modifiers:04X}, vk=0x{vk:02X}")
        else:
            print(f"  ✗ {key_str} -> modifiers=0x{modifiers:04X} (expected 0x{expected_mod:04X}), vk=0x{vk:02X} (expected 0x{expected_vk:02X})")
            all_passed = False

    return all_passed

class TestWindow(QWidget):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("热键测试")
        self.setFixedSize(400, 200)

        layout = QVBoxLayout(self)
        self.label = QLabel("按下 Ctrl+O 测试热键\n按下 Ctrl+Shift+C 测试第二个热键\n按下 Ctrl+F 测试第三个热键")
        self.label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(self.label)

        self.hotkey_controller = HotkeyController(self)
        self.trigger_count = 0

        # 注册测试热键
        result1 = self.hotkey_controller.register_shortcut("Ctrl+O", self.on_hotkey1)
        result2 = self.hotkey_controller.register_shortcut("Ctrl+Shift+C", self.on_hotkey2)
        result3 = self.hotkey_controller.register_shortcut("Ctrl+F", self.on_hotkey3)

        print(f"\n热键注册结果:")
        print(f"  Ctrl+O: {'✓ 成功' if result1 else '✗ 失败'}")
        print(f"  Ctrl+Shift+C: {'✓ 成功' if result2 else '✗ 失败'}")
        print(f"  Ctrl+F: {'✓ 成功' if result3 else '✗ 失败'}")

        # 5秒后自动关闭
        QTimer.singleShot(5000, self.close)

    def on_hotkey1(self):
        self.trigger_count += 1
        self.label.setText(f"✓ 热键1触发! (Ctrl+O)\n总触发次数: {self.trigger_count}")
        print(f"  热键1触发! (Ctrl+O)")

    def on_hotkey2(self):
        self.trigger_count += 1
        self.label.setText(f"✓ 热键2触发! (Ctrl+Shift+C)\n总触发次数: {self.trigger_count}")
        print(f"  热键2触发! (Ctrl+Shift+C)")

    def on_hotkey3(self):
        self.trigger_count += 1
        self.label.setText(f"✓ 热键3触发! (Ctrl+F)\n总触发次数: {self.trigger_count}")
        print(f"  热键3触发! (Ctrl+F)")

def main():
    print("=" * 50)
    print("ClipMaster Pro 热键修复测试")
    print("=" * 50)

    # 测试解析函数
    if not test_parse_hotkey():
        print("\n解析测试失败，请检查代码!")
        return 1

    # 创建 Qt 应用测试实际热键
    print("\n" + "=" * 50)
    print("启动热键测试窗口...")
    print("请尝试按下以下热键:")
    print("  - Ctrl+O")
    print("  - Ctrl+Shift+C")
    print("  - Ctrl+F")
    print("\n窗口将在5秒后自动关闭")
    print("=" * 50)

    app = QApplication(sys.argv)
    window = TestWindow()
    window.show()

    return app.exec()

if __name__ == "__main__":
    sys.exit(main())
