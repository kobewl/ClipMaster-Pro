@echo off
cd /d "%~dp0"
powershell Start-Process python -ArgumentList "clipboard_assistant.py" -Verb RunAs -Wait 