use objc2::rc::Retained;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::NSString;

/// 获取当前活跃应用的名称
pub fn get_active_app_name() -> Option<String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let name: Retained<NSString> = app.localizedName()?;
    Some(name.to_string())
}
