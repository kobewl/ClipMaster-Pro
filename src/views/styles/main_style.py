"""现代化UI样式定义 - 跨平台桌面风格"""

import platform


_FONT_STACK = (
    "'PingFang SC', 'Helvetica Neue', 'Hiragino Sans GB', sans-serif"
    if platform.system() == "Darwin"
    else "'Segoe UI Variable', 'Segoe UI', 'Microsoft YaHei', sans-serif"
)

class MainStyle:
    """主要样式定义 - 现代亮色主题"""
    
    COLORS = {
        'primary': '#0A84FF',
        'primary_hover': '#409CFF',
        'primary_light': '#EAF4FF',
        'secondary': '#516074',
        'success': '#0F9D58',
        'warning': '#C97A10',
        'danger': '#C42B1C',
        'background': '#EEF3F8',
        'surface': '#FFFFFF',
        'surface_alt': '#F8FBFF',
        'border': '#D8E3EE',
        'border_strong': '#B7C9DA',
        'text_primary': '#152033',
        'text_secondary': '#516074',
        'text_muted': '#7D8EA3',
    }
    
    MAIN_WIDGET = """
        QMainWindow {
            background-color: #EEF3F8;
        }

        #mainWidget {
            background-color: qlineargradient(
                x1: 0, y1: 0, x2: 1, y2: 1,
                stop: 0 #F4F8FC,
                stop: 0.55 #EDF4FB,
                stop: 1 #E8EFF7
            );
            border: none;
        }

        #borderFrame {
            background-color: qlineargradient(
                x1: 0, y1: 0, x2: 0, y2: 1,
                stop: 0 #FFFFFF,
                stop: 1 #F8FBFF
            );
            border: none;
            border-radius: 14px;
        }

        #contentWidget {
            background-color: transparent;
            border: none;
        }
    """

    TITLE_BAR = """
        #titleBar {
            background-color: qlineargradient(
                x1: 0, y1: 0, x2: 1, y2: 0,
                stop: 0 rgba(10, 132, 255, 0.14),
                stop: 1 rgba(90, 200, 250, 0.03)
            );
            padding: 10px 14px;
            border-bottom: 1px solid #D8E3EE;
        }
        
        #titleLabel {
            font-family: """ + _FONT_STACK + """;
            font-size: 15px;
            font-weight: 700;
            color: #152033;
            letter-spacing: 0.3px;
            padding-left: 6px;
        }
    """
    
    SEARCH_CONTAINER = """
        #searchContainer {
            background-color: rgba(255, 255, 255, 0.88);
            border: 1px solid #D8E3EE;
            border-radius: 10px;
            padding: 5px 10px;
            margin: 2px 0;
        }
        
        #searchContainer:focus-within {
            border: 1px solid #0A84FF;
            background-color: #FFFFFF;
        }
        
        #searchContainer QLineEdit {
            border: none;
            background: transparent;
            padding: 4px 4px;
            font-size: 13px;
            font-family: """ + _FONT_STACK + """;
            color: #152033;
            min-width: 200px;
        }
        
        #searchContainer QLineEdit:focus {
            outline: none;
        }
        
        #searchContainer QLineEdit::placeholder {
            color: #999999;
        }
    """
    
    BUTTONS = """
        QPushButton {
            border-radius: 9px;
            font-size: 13px;
            padding: 6px 12px;
            font-weight: 500;
            border: 1px solid transparent;
            background-color: transparent;
            font-family: """ + _FONT_STACK + """;
        }
        
        /* 图标按钮 */
        #iconButton {
            color: #516074;
            min-width: 32px;
            min-height: 32px;
            padding: 6px;
            border-radius: 9px;
            background-color: rgba(255, 255, 255, 0.55);
        }
        
        #iconButton:hover {
            background-color: #FFFFFF;
            color: #152033;
            border: 1px solid #D8E3EE;
        }
        
        #iconButton:checked {
            background-color: #EAF4FF;
            color: #0A84FF;
            border: 1px solid #B7C9DA;
        }
        
        /* 主要按钮 */
        #primaryButton {
            background-color: #0067C0;
            color: white;
            border: 1px solid #0067C0;
        }
        
        #primaryButton:hover {
            background-color: #1975C5;
        }
        
        /* 次要按钮 */
        #secondaryButton {
            background-color: #FFFFFF;
            color: #152033;
            border: 1px solid #D8E3EE;
        }
        
        #secondaryButton:hover {
            background-color: #F8FBFF;
        }
        
        /* 危险按钮 */
        #dangerButton {
            background-color: transparent;
            color: #C42B1C;
        }
        
        #dangerButton:hover {
            background-color: #FEECEB;
        }
        
        /* 关闭按钮 */
        #closeButton {
            background-color: transparent;
            color: #516074;
            font-size: 14px;
            min-width: 32px;
            min-height: 32px;
            padding: 4px;
            border-radius: 9px;
        }
        
        #closeButton:hover {
            background-color: #C42B1C;
            color: white;
        }
    """
    
    HISTORY_LIST = """
        #historyList {
            border: none;
            background-color: transparent;
            padding: 4px;
            outline: none;
        }
        
        #historyList::item {
            margin: 2px 4px;
            border-radius: 6px;
            background-color: transparent;
            border: none;
            min-height: 36px;
        }
        
        #historyList::item:hover {
            background-color: #F0F0F0;
        }
        
        #historyList::item:selected {
            background-color: transparent;
            border: none;
        }
        
        QScrollBar:vertical {
            background-color: transparent;
            width: 4px;
            margin: 2px 2px 2px 0;
        }
        
        QScrollBar::handle:vertical {
            background-color: #CCCCCC;
            border-radius: 2px;
            min-height: 24px;
        }
        
        QScrollBar::handle:vertical:hover {
            background-color: #999999;
        }
        
        QScrollBar::add-line:vertical,
        QScrollBar::sub-line:vertical,
        QScrollBar::add-page:vertical,
        QScrollBar::sub-page:vertical {
            background: none;
            height: 0px;
        }
    """
    
    SEPARATOR = """
        #separator {
            background-color: #E5E5E5;
            height: 1px;
            margin: 4px 12px;
        }
    """
    
    TOOLBAR = """
        #toolbar {
            background-color: transparent;
            padding: 8px 14px 6px 14px;
            border-bottom: 1px solid transparent;
        }
        
        #filterButton {
            background-color: rgba(255, 255, 255, 0.45);
            color: #516074;
            border: 1px solid #D8E3EE;
            border-radius: 9px;
            padding: 6px 12px;
            font-size: 13px;
        }
        
        #filterButton:hover {
            background-color: #FFFFFF;
        }
        
        #filterButton:checked {
            background-color: #EAF4FF;
            color: #0A84FF;
            border: 1px solid #B7C9DA;
        }
    """
    
    STATUS_BAR = """
        #statusBar {
            background-color: rgba(10, 132, 255, 0.06);
            color: #516074;
            font-size: 12px;
            font-family: """ + _FONT_STACK + """;
            padding: 9px 14px;
            border-top: 1px solid #D8E3EE;
            border-bottom-left-radius: 14px;
            border-bottom-right-radius: 14px;
        }
    """
    
    @classmethod
    def get_all_styles(cls) -> str:
        """获取所有样式"""
        return "\n".join([
            cls.MAIN_WIDGET,
            cls.TITLE_BAR,
            cls.SEARCH_CONTAINER,
            cls.BUTTONS,
            cls.HISTORY_LIST,
            cls.SEPARATOR,
            cls.TOOLBAR,
            cls.STATUS_BAR,
        ])


class DarkStyle:
    """现代暗黑模式样式 - Fluent Design"""
    
    COLORS = {
        'primary': '#4CC2FF',
        'primary_hover': '#47B1E8',
        'primary_light': '#202A35',
        'secondary': '#A0AABF',
        'success': '#6CCB5F',
        'warning': '#FCE100',
        'danger': '#FF99A4',
        'background': '#111827',
        'surface': '#18212F',
        'surface_alt': '#101827',
        'border': '#29415B',
        'border_strong': '#355575',
        'text_primary': '#FFFFFF',
        'text_secondary': '#A0AABF',
        'text_muted': '#7A7A7A',
    }
    
    MAIN_WIDGET = """
        QMainWindow {
            background-color: #111827;
        }

        #mainWidget {
            background-color: qlineargradient(
                x1: 0, y1: 0, x2: 1, y2: 1,
                stop: 0 #111827,
                stop: 0.5 #132033,
                stop: 1 #0D1727
            );
            border: none;
        }

        #borderFrame {
            background-color: qlineargradient(
                x1: 0, y1: 0, x2: 0, y2: 1,
                stop: 0 #1A2433,
                stop: 1 #121B29
            );
            border: none;
            border-radius: 14px;
        }

        #contentWidget {
            background-color: transparent;
            border: none;
        }
    """

    TITLE_BAR = """
        #titleBar {
            background-color: qlineargradient(
                x1: 0, y1: 0, x2: 1, y2: 0,
                stop: 0 rgba(76, 194, 255, 0.18),
                stop: 1 rgba(24, 33, 47, 0.12)
            );
            padding: 10px 14px;
            border-bottom: 1px solid #29415B;
        }
        
        #titleLabel {
            font-family: """ + _FONT_STACK + """;
            font-size: 15px;
            font-weight: 700;
            color: #FFFFFF;
            letter-spacing: 0.3px;
            padding-left: 6px;
        }
    """
    
    SEARCH_CONTAINER = """
        #searchContainer {
            background-color: rgba(24, 33, 47, 0.92);
            border: 1px solid #29415B;
            border-radius: 10px;
            padding: 5px 10px;
            margin: 2px 0;
        }
        
        #searchContainer:focus-within {
            border: 1px solid #4CC2FF;
            background-color: #1A2433;
        }
        
        #searchContainer QLineEdit {
            border: none;
            background: transparent;
            padding: 4px 4px;
            font-size: 13px;
            font-family: """ + _FONT_STACK + """;
            color: #FFFFFF;
            min-width: 200px;
        }
        
        #searchContainer QLineEdit:focus {
            outline: none;
        }
        
        #searchContainer QLineEdit::placeholder {
            color: #7A7A7A;
        }
    """
    
    BUTTONS = """
        QPushButton {
            border-radius: 9px;
            font-size: 13px;
            padding: 6px 12px;
            font-weight: 500;
            border: 1px solid transparent;
            background-color: transparent;
            font-family: """ + _FONT_STACK + """;
        }
        
        /* 图标按钮 */
        #iconButton {
            color: #A0AABF;
            min-width: 32px;
            min-height: 32px;
            padding: 6px;
            border-radius: 9px;
            background-color: rgba(26, 36, 51, 0.76);
        }
        
        #iconButton:hover {
            background-color: #213247;
            color: #FFFFFF;
            border: 1px solid #29415B;
        }
        
        #iconButton:checked {
            background-color: #203347;
            color: #4CC2FF;
            border: 1px solid #355575;
        }
        
        /* 主要按钮 */
        #primaryButton {
            background-color: #4CC2FF;
            color: #000000;
            border: 1px solid #4CC2FF;
        }
        
        #primaryButton:hover {
            background-color: #47B1E8;
        }
        
        /* 次要按钮 */
        #secondaryButton {
            background-color: #18212F;
            color: #FFFFFF;
            border: 1px solid #29415B;
        }
        
        #secondaryButton:hover {
            background-color: #203347;
        }
        
        /* 危险按钮 */
        #dangerButton {
            background-color: transparent;
            color: #FF99A4;
        }
        
        #dangerButton:hover {
            background-color: #4A262A;
        }
        
        /* 关闭按钮 */
        #closeButton {
            background-color: transparent;
            color: #A0AABF;
            font-size: 14px;
            min-width: 32px;
            min-height: 32px;
            padding: 4px;
            border-radius: 9px;
        }
        
        #closeButton:hover {
            background-color: #C42B1C;
            color: white;
        }
    """
    
    HISTORY_LIST = """
        #historyList {
            border: none;
            background-color: transparent;
            padding: 4px;
            outline: none;
        }
        
        #historyList::item {
            margin: 2px 4px;
            border-radius: 6px;
            background-color: transparent;
            border: none;
            min-height: 36px;
        }
        
        #historyList::item:hover {
            background-color: #2D2D2D;
        }
        
        #historyList::item:selected {
            background-color: transparent;
            border: none;
        }
        
        QScrollBar:vertical {
            background-color: transparent;
            width: 4px;
            margin: 2px 2px 2px 0;
        }
        
        QScrollBar::handle:vertical {
            background-color: #555555;
            border-radius: 2px;
            min-height: 24px;
        }
        
        QScrollBar::handle:vertical:hover {
            background-color: #777777;
        }
        
        QScrollBar::add-line:vertical,
        QScrollBar::sub-line:vertical,
        QScrollBar::add-page:vertical,
        QScrollBar::sub-page:vertical {
            background: none;
            height: 0px;
        }
    """
    
    SEPARATOR = """
        #separator {
            background-color: #333333;
            height: 1px;
            margin: 4px 12px;
        }
    """
    
    TOOLBAR = """
        #toolbar {
            background-color: transparent;
            padding: 8px 14px 6px 14px;
            border-bottom: 1px solid transparent;
        }
        
        #filterButton {
            background-color: rgba(26, 36, 51, 0.72);
            color: #A0AABF;
            border: 1px solid #29415B;
            border-radius: 9px;
            padding: 6px 12px;
            font-size: 13px;
        }
        
        #filterButton:hover {
            background-color: #213247;
        }
        
        #filterButton:checked {
            background-color: #203347;
            color: #4CC2FF;
            border: 1px solid #355575;
        }
    """
    
    STATUS_BAR = """
        #statusBar {
            background-color: rgba(76, 194, 255, 0.08);
            color: #A0AABF;
            font-size: 12px;
            font-family: """ + _FONT_STACK + """;
            padding: 9px 14px;
            border-top: 1px solid #29415B;
            border-bottom-left-radius: 14px;
            border-bottom-right-radius: 14px;
        }
    """
    
    @classmethod
    def get_all_styles(cls) -> str:
        """获取所有样式"""
        return "\n".join([
            cls.MAIN_WIDGET,
            cls.TITLE_BAR,
            cls.SEARCH_CONTAINER,
            cls.BUTTONS,
            cls.HISTORY_LIST,
            cls.SEPARATOR,
            cls.TOOLBAR,
            cls.STATUS_BAR,
        ])


class StyleManager:
    """样式管理器"""
    
    _current_style = None
    _is_dark_mode = False
    
    @classmethod
    def get_style(cls, is_dark_mode: bool = False) -> str:
        """获取当前主题的样式"""
        cls._is_dark_mode = is_dark_mode
        cls._current_style = DarkStyle.get_all_styles() if is_dark_mode else MainStyle.get_all_styles()
        return cls._current_style
    
    @classmethod
    def get_colors(cls):
        """获取当前主题颜色"""
        return DarkStyle.COLORS if cls._is_dark_mode else MainStyle.COLORS
    
    @classmethod
    def is_dark_mode(cls) -> bool:
        """获取当前是否为暗色模式"""
        return cls._is_dark_mode
