//! Popup 窗口定位的纯函数几何计算（FRB-003）。
//!
//! 设计目标：Popup 自然出现在菜单栏/当前屏幕顶部附近（贴工作区上沿，
//! 水平方向优先对齐托盘图标锚点，无锚点时靠右），并通过边界夹取保证
//! 窗口完整落在工作区内，不越屏。

/// 工作区矩形（物理像素）。macOS 上 work area 已排除菜单栏与 Dock。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkArea {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

fn clamp_with_fallback(value: f64, lo: f64, hi: f64) -> f64 {
    // 窗口比工作区还宽/高时 hi < lo，退回到 lo（至少保证左/上边不越界）
    value.clamp(lo, hi.max(lo))
}

/// 计算 Popup 左上角坐标（物理像素）。
///
/// - 纵坐标：贴工作区顶部 + `margin`（即菜单栏下沿附近）；
/// - 横坐标：有锚点时让窗口水平中心对齐锚点，无锚点时靠右对齐；
/// - 两个方向都夹取在工作区内（四边留白 `margin`）。
pub fn popup_origin(
    area: WorkArea,
    popup_width: f64,
    popup_height: f64,
    anchor_x: Option<f64>,
    margin: f64,
) -> (f64, f64) {
    let x_lo = area.x + margin;
    let x_hi = area.x + area.width - popup_width - margin;
    let y_lo = area.y + margin;
    let y_hi = area.y + area.height - popup_height - margin;

    let raw_x = match anchor_x {
        Some(ax) => ax - popup_width / 2.0,
        None => x_hi, // 菜单栏工具习惯：无锚点时贴右上角
    };
    (
        clamp_with_fallback(raw_x, x_lo, x_hi),
        clamp_with_fallback(y_lo, y_lo, y_hi),
    )
}

/// 返回包含指定点的工作区下标；点不在任何工作区内时返回 None（调用方回退主屏）。
pub fn find_work_area(areas: &[WorkArea], point_x: f64, point_y: f64) -> Option<usize> {
    areas.iter().position(|a| {
        point_x >= a.x && point_x < a.x + a.width && point_y >= a.y && point_y < a.y + a.height
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARGIN: f64 = 8.0;

    fn area() -> WorkArea {
        // 模拟 1440x875 工作区（已排除菜单栏），从 (0, 25) 开始
        WorkArea {
            x: 0.0,
            y: 25.0,
            width: 1440.0,
            height: 875.0,
        }
    }

    #[test]
    fn 锚点居中_窗口水平中心对齐托盘图标() {
        let (x, y) = popup_origin(area(), 280.0, 460.0, Some(1200.0), MARGIN);
        assert_eq!(x, 1200.0 - 140.0);
        assert_eq!(y, 25.0 + MARGIN); // 贴菜单栏下沿
    }

    #[test]
    fn 锚点贴近右边缘_夹取不越屏() {
        let (x, _) = popup_origin(area(), 280.0, 460.0, Some(1439.0), MARGIN);
        assert_eq!(x, 1440.0 - 280.0 - MARGIN);
    }

    #[test]
    fn 锚点贴近左边缘_夹取不越屏() {
        let (x, _) = popup_origin(area(), 280.0, 460.0, Some(4.0), MARGIN);
        assert_eq!(x, MARGIN);
    }

    #[test]
    fn 无锚点_靠右对齐屏幕顶部() {
        let (x, y) = popup_origin(area(), 280.0, 460.0, None, MARGIN);
        assert_eq!(x, 1440.0 - 280.0 - MARGIN);
        assert_eq!(y, 25.0 + MARGIN);
    }

    #[test]
    fn 工作区带偏移量_结果随之平移() {
        let shifted = WorkArea {
            x: 1440.0,
            y: 0.0,
            width: 1920.0,
            height: 1055.0,
        };
        let (x, y) = popup_origin(shifted, 280.0, 460.0, Some(1600.0), MARGIN);
        assert_eq!(x, 1600.0 - 140.0);
        assert_eq!(y, MARGIN);
    }

    #[test]
    fn 窗口比工作区宽_退化到左边界不panic() {
        let tiny = WorkArea {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };
        let (x, y) = popup_origin(tiny, 280.0, 460.0, Some(100.0), MARGIN);
        assert_eq!(x, MARGIN);
        assert_eq!(y, MARGIN);
    }

    #[test]
    fn 查找包含锚点的工作区() {
        let areas = [
            area(),
            WorkArea {
                x: 1440.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
        ];
        assert_eq!(find_work_area(&areas, 100.0, 100.0), Some(0));
        assert_eq!(find_work_area(&areas, 1500.0, 100.0), Some(1));
        assert_eq!(find_work_area(&areas, 1440.0, 0.0), Some(1)); // 边界归属右屏
        assert_eq!(find_work_area(&areas, -10.0, 100.0), None);
        assert_eq!(find_work_area(&areas, 100.0, 24.0), None); // 菜单栏区域不属于工作区
        assert_eq!(find_work_area(&[], 0.0, 0.0), None);
    }
}
