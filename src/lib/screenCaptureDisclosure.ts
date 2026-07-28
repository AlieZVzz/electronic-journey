export const screenCaptureDisclosure = {
  introduction:
    "为按计划自动截取当前主显示器，Electronic Journey 会请求 macOS 的程序化屏幕访问，不会在每次截图前打开系统的私密窗口选择器。",
  screenAccess:
    "授权后，应用具备读取屏幕上可见内容的能力，其中可能包含私人或敏感信息。只有你主动开始记录后，应用才会按计划截图；暂停或停止会终止后续截图。",
  audio:
    "macOS 的系统提示可能同时提到屏幕和音频。Electronic Journey 当前只获取静态屏幕像素，不采集、保存或上传麦克风或系统音频。",
} as const;

export function canRequestScreenCapturePermission(
  acknowledged: boolean,
  loading: boolean,
): boolean {
  return acknowledged && !loading;
}
