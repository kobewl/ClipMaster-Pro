import { useEffect, useRef } from "react";
import { onCaptureChanged, onOpenSettings } from "@/lib/events";

interface Options {
  /** 后端（托盘菜单等）改了采集开关，参数是新状态。 */
  onCaptureChanged: (captureEnabled: boolean) => void;
  /** 托盘「打开设置」：窗口已被后端拉起并聚焦，这里只负责打开面板。 */
  onOpenSettings: () => void;
}

/**
 * 托盘菜单触发的全局事件。
 *
 * 回调收进 ref：监听只挂载一次（依赖数组为空），每次事件都转发给
 * **最新的**回调 —— 不会因为组件重渲染被反复拆卸，也不会闭包过期。
 */
export function useTrayEvents(options: Options) {
  const callbacksRef = useRef(options);
  callbacksRef.current = options;

  useEffect(() => {
    let disposed = false;
    const unlisteners = [
      onCaptureChanged((enabled) => {
        if (!disposed) callbacksRef.current.onCaptureChanged(enabled);
      }),
      onOpenSettings(() => {
        if (!disposed) callbacksRef.current.onOpenSettings();
      }),
    ];
    return () => {
      disposed = true;
      unlisteners.forEach((pending) => pending.then((fn) => fn()).catch(() => {}));
    };
  }, []);
}
