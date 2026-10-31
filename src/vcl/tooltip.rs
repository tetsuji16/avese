//! Classic yellow tooltips shown about 500 ms after the pointer settles on a control.
use egui::{Color32, Context, Id, LayerId, Order, Pos2, Rect, Vec2};

#[derive(Clone, Copy)]
struct State {
    id: Id,
    since: f64,
    pos: Pos2,
}

const DELAY: f64 = 0.5;

/// Call once per viewport per frame with the hovered control (id, hint text), if any.
pub fn update(ctx: &Context, hover: Option<(Id, &str)>) {
    let key = Id::new(("avese_tooltip", ctx.viewport_id()));
    let now = ctx.input(|i| i.time);
    let (ptr, down, moved) = ctx.input(|i| (i.pointer.hover_pos(), i.pointer.any_down(), i.pointer.delta() != Vec2::ZERO));
    let prev: Option<State> = ctx.data(|d| d.get_temp(key));
    let (Some((id, text)), Some(ptr)) = (hover, ptr) else {
        ctx.data_mut(|d| d.remove::<State>(key));
        return;
    };
    if text.is_empty() {
        ctx.data_mut(|d| d.remove::<State>(key));
        return;
    }
    let mut st = match prev {
        Some(s) if s.id == id => s,
        _ => State { id, since: now, pos: ptr },
    };
    if down {
        st.since = f64::INFINITY; // hidden until the pointer enters another control
    } else if moved && now - st.since < DELAY {
        st.since = now;
        st.pos = ptr;
    }
    ctx.data_mut(|d| d.insert_temp(key, st));
    if st.since.is_infinite() {
        return;
    }
    if now - st.since < DELAY {
        ctx.request_repaint_after(std::time::Duration::from_millis(60));
        return;
    }
    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, key));
    let font = crate::fonts::ui();
    let galley = painter.layout_no_wrap(text.to_string(), font, Color32::BLACK);
    let size = galley.size() + Vec2::new(8.0, 6.0);
    let screen = ctx.content_rect();
    let mut pos = Pos2::new(st.pos.x + 2.0, st.pos.y + 20.0).round();
    if pos.x + size.x > screen.max.x {
        pos.x = (screen.max.x - size.x).max(0.0);
    }
    if pos.y + size.y > screen.max.y {
        pos.y = (st.pos.y - size.y - 2.0).max(0.0);
    }
    let r = Rect::from_min_size(pos, size);
    super::fill(&painter, r, Color32::from_rgb(255, 255, 225));
    super::rect_outline(&painter, r, Color32::BLACK);
    painter.galley(pos + Vec2::new(4.0, 3.0), galley, Color32::BLACK);
}
