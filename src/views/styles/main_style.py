"""现代化UI样式定义 - Fluent Design 风格"""

class MainStyle:
    """主要样式定义 - 现代亮色主题"""
    
    COLORS = {
        'primary': '#0067C0',
        'primary_hover': '#1975C5',
        'primary_light': '#F3F9FF',
        'secondary': '#5D6471',
        'success': '#0F7B0F',
        'warning': '#9D5D00',
        'danger': '#C42B1C',
        'background': '#FAFAFA',
        'surface': '#FFFFFF',
        'border': '#E5E5E5',
        'text_primary': '#1A1A1A',
        'text_secondary': '#5D6471',
        'text_muted': '#999999',
    }
    
    MAIN_WIDGET = """
        QMainWindow {
            background-color: transparent;
        }
        
        #mainWidget {
            background-color: #FAFAFA;
            border: 1px solid #E5E5E5;
            border-radius: 8px;
        }
    """
    
    TITLE_BAR = """
        #titleBar {
            background-color: transparent;
            padding: 8px 12px;
            border-bottom: 1px solid #E5E5E5;
        }
        
        #titleLabel {
            font-family: 'Segoe UI Variable', 'Segoe UI', 'Microsoft YaHei', sans-serif;
            font-size: 14px;
            font-weight: 600;
            color: #1A1A1A;
            padding-left: 4px;
        }
    """
    
    SEARCH_CONTAINER = """
        #searchContainer {
            background-color: #FFFFFF;
            border: 1px solid #E5E5E5;
            border-radius: 6px;
            padding: 4px 8px;
            margin: 4px 0;
        }
        
        #searchContainer:focus-within {
            border-bottom: 2px solid #0067C0;
            padding-bottom: 3px;
        }
        
        #searchContainer QLineEdit {
            border: none;
            background: transparent;
            padding: 4px 4px;
            font-size: 13px;
            color: #1A1A1A;
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
            border-radius: 6px;
            font-size: 13px;
            padding: 6px 12px;
            font-weight: 400;
            border: 1px solid transparent;
            background-color: transparent;
        }
        
        /* 图标按钮 */
        #iconButton {
            color: #5D6471;
            min-width: 32px;
            min-height: 32px;
            padding: 6px;
            border-radius: 6px;
            background-color: transparent;
        }
        
        #iconButton:hover {
            background-color: #F0F0F0;
            color: #1A1A1A;
        }
        
        #iconButton:checked {
            background-color: #FFFFFF;
            color: #0067C0;
            border: 1px solid #E5E5E5;
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
            color: #1A1A1A;
            border: 1px solid #E5E5E5;
            border-bottom-color: #CCCCCC;
        }
        
        #secondaryButton:hover {
            background-color: #F6F6F6;
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
            color: #5D6471;
            font-size: 14px;
            min-width: 32px;
            min-height: 32px;
            padding: 4px;
            border-radius: 6px;
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
            padding: 4px 12px;
            border-bottom: 1px solid transparent;
        }
        
        #filterButton {
            background-color: transparent;
            color: #5D6471;
            border: 1px solid transparent;
            border-radius: 6px;
            padding: 6px 10px;
            font-size: 13px;
        }
        
        #filterButton:hover {
            background-color: #F0F0F0;
        }
        
        #filterButton:checked {
            background-color: #FFFFFF;
            color: #0067C0;
            border: 1px solid #E5E5E5;
        }
    """
    
    STATUS_BAR = """
        #statusBar {
            background-color: #FAFAFA;
            color: #5D6471;
            font-size: 12px;
            padding: 8px 12px;
            border-top: 1px solid #E5E5E5;
            border-bottom-left-radius: 8px;
            border-bottom-right-radius: 8px;
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
        'background': '#202020',
        'surface': '#2D2D2D',
        'border': '#434343',
        'text_primary': '#FFFFFF',
        'text_secondary': '#A0AABF',
        'text_muted': '#7A7A7A',
    }
    
    MAIN_WIDGET = """
        QMainWindow {
            background-color: transparent;
        }
        
        #mainWidget {
            background-color: #202020;
            border: 1px solid #333333;
            border-radius: 8px;
        }
    """
    
    TITLE_BAR = """
        #titleBar {
            background-color: transparent;
            padding: 8px 12px;
            border-bottom: 1px solid #333333;
        }
        
        #titleLabel {
            font-family: 'Segoe UI Variable', 'Segoe UI', 'Microsoft YaHei', sans-serif;
            font-size: 14px;
            font-weight: 600;
            color: #FFFFFF;
            padding-left: 4px;
        }
    """
    
    SEARCH_CONTAINER = """
        #searchContainer {
            background-color: #2D2D2D;
            border: 1px solid #333333;
            border-radius: 6px;
            padding: 4px 8px;
            margin: 4px 0;
            border-bottom: 1px solid #555555;
        }
        
        #searchContainer:focus-within {
            border-bottom: 2px solid #4CC2FF;
            padding-bottom: 3px;
        }
        
        #searchContainer QLineEdit {
            border: none;
            background: transparent;
            padding: 4px 4px;
            font-size: 13px;
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
            border-radius: 6px;
            font-size: 13px;
            padding: 6px 12px;
            font-weight: 400;
            border: 1px solid transparent;
            background-color: transparent;
        }
        
        /* 图标按钮 */
        #iconButton {
            color: #A0AABF;
            min-width: 32px;
            min-height: 32px;
            padding: 6px;
            border-radius: 6px;
            background-color: transparent;
        }
        
        #iconButton:hover {
            background-color: #2D2D2D;
            color: #FFFFFF;
        }
        
        #iconButton:checked {
            background-color: #333333;
            color: #4CC2FF;
            border: 1px solid #434343;
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
            background-color: #2D2D2D;
            color: #FFFFFF;
            border: 1px solid #333333;
            border-top-color: #444444;
        }
        
        #secondaryButton:hover {
            background-color: #383838;
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
            border-radius: 6px;
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
            padding: 4px 12px;
            border-bottom: 1px solid transparent;
        }
        
        #filterButton {
            background-color: transparent;
            color: #A0AABF;
            border: 1px solid transparent;
            border-radius: 6px;
            padding: 6px 10px;
            font-size: 13px;
        }
        
        #filterButton:hover {
            background-color: #2D2D2D;
        }
        
        #filterButton:checked {
            background-color: #333333;
            color: #4CC2FF;
            border: 1px solid #434343;
        }
    """
    
    STATUS_BAR = """
        #statusBar {
            background-color: #202020;
            color: #A0AABF;
            font-size: 12px;
            padding: 8px 12px;
            border-top: 1px solid #333333;
            border-bottom-left-radius: 8px;
            border-bottom-right-radius: 8px;
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