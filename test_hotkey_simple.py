"""Simple test for hotkey fix"""
import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'src'))

from PyQt6.QtWidgets import QApplication, QLabel, QVBoxLayout, QWidget
from PyQt6.QtCore import Qt, QTimer
from controllers.hotkey_controller import HotkeyController

class TestWindow(QWidget):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("Hotkey Test")
        self.setFixedSize(400, 200)

        layout = QVBoxLayout(self)
        self.label = QLabel("Press Ctrl+O to test hotkey\nPress Ctrl+Shift+C for second hotkey\nPress Ctrl+F for third hotkey")
        self.label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(self.label)

        self.hotkey_controller = HotkeyController(self)
        self.trigger_count = 0

        # Register test hotkeys
        result1 = self.hotkey_controller.register_shortcut("Ctrl+O", self.on_hotkey1)
        result2 = self.hotkey_controller.register_shortcut("Ctrl+Shift+C", self.on_hotkey2)
        result3 = self.hotkey_controller.register_shortcut("Ctrl+F", self.on_hotkey3)

        print(f"Hotkey registration results:")
        print(f"  Ctrl+O: {'SUCCESS' if result1 else 'FAILED'}")
        print(f"  Ctrl+Shift+C: {'SUCCESS' if result2 else 'FAILED'}")
        print(f"  Ctrl+F: {'SUCCESS' if result3 else 'FAILED'}")

        # Auto close after 10 seconds
        QTimer.singleShot(10000, self.close)

    def on_hotkey1(self):
        self.trigger_count += 1
        self.label.setText(f"[OK] Hotkey 1 triggered! (Ctrl+O)\nTotal: {self.trigger_count}")
        print(f"  [TRIGGERED] Hotkey 1 (Ctrl+O)")

    def on_hotkey2(self):
        self.trigger_count += 1
        self.label.setText(f"[OK] Hotkey 2 triggered! (Ctrl+Shift+C)\nTotal: {self.trigger_count}")
        print(f"  [TRIGGERED] Hotkey 2 (Ctrl+Shift+C)")

    def on_hotkey3(self):
        self.trigger_count += 1
        self.label.setText(f"[OK] Hotkey 3 triggered! (Ctrl+F)\nTotal: {self.trigger_count}")
        print(f"  [TRIGGERED] Hotkey 3 (Ctrl+F)")

def main():
    print("=" * 50)
    print("ClipMaster Pro Hotkey Test")
    print("=" * 50)
    print("\nTry pressing these hotkeys:")
    print("  - Ctrl+O")
    print("  - Ctrl+Shift+C")
    print("  - Ctrl+F")
    print("\nWindow will close automatically after 10 seconds")
    print("=" * 50)

    app = QApplication(sys.argv)
    window = TestWindow()
    window.show()

    return app.exec()

if __name__ == "__main__":
    sys.exit(main())
