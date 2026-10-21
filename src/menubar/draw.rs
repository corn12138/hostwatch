//! Native drawing for cards, bars, progress boxes and charts, using semantic system colors so
//! light and dark appearances follow the system.

use super::*;

pub(super) fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

pub(super) fn tone_color(tone: Tone) -> Retained<NSColor> {
    match tone {
        Tone::Ok | Tone::Recovering => NSColor::systemGreenColor(),
        Tone::Warning => NSColor::systemOrangeColor(),
        Tone::Critical => NSColor::systemRedColor(),
        Tone::Loading => NSColor::systemBlueColor(),
        Tone::Unknown | Tone::Paused | Tone::Neutral => NSColor::secondaryLabelColor(),
    }
}

pub(super) fn symbol(tone: Tone) -> &'static str {
    match tone {
        Tone::Ok => "checkmark.circle.fill",
        Tone::Warning => "exclamationmark.triangle.fill",
        Tone::Critical => "exclamationmark.octagon.fill",
        Tone::Unknown => "questionmark.circle",
        Tone::Paused => "pause.circle.fill",
        Tone::Recovering => "circle.lefthalf.filled",
        Tone::Loading => "circle.dotted",
        Tone::Neutral => "info.circle",
    }
}

pub(super) fn text_attributes(
    size: f64,
    color: &NSColor,
) -> Retained<NSDictionary<NSAttributedStringKey, AnyObject>> {
    let font = NSFont::systemFontOfSize(size);
    let values: [&AnyObject; 2] = [font.as_ref(), color.as_ref()];
    // SAFETY: AppKit string attribute keys are immutable static strings.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    NSDictionary::from_slices(&keys, &values)
}

/// Draws `text` with its left, center or right edge at `x` (`align` = 0, 1, 2).
pub(super) fn draw_text(text: &str, x: f64, y: f64, size: f64, color: &NSColor, align: u8) {
    let string = NSString::from_str(text);
    let attributes = text_attributes(size, color);
    // SAFETY: the attribute dictionary holds an NSFont and an NSColor under the matching keys.
    unsafe {
        let width = string.sizeWithAttributes(Some(&attributes)).width;
        let x = match align {
            0 => x,
            1 => x - width / 2.0,
            _ => x - width,
        };
        string.drawAtPoint_withAttributes(NSPoint::new(x, y), Some(&attributes));
    }
}

pub(super) fn dashed(path: &NSBezierPath) {
    let pattern = [4.0f64, 3.0];
    // SAFETY: the pattern pointer is valid for `count` elements during the call.
    unsafe { path.setLineDash_count_phase(pattern.as_ptr(), 2, 0.0) };
}

pub(super) fn draw(shape: &Shape, bounds: NSRect) {
    let (w, h) = (bounds.size.width, bounds.size.height);
    match shape {
        Shape::Card(tone) => {
            let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                rect(0.5, 0.5, w - 1.0, h - 1.0),
                8.0,
                8.0,
            );
            NSColor::controlBackgroundColor()
                .colorWithAlphaComponent(0.7)
                .setFill();
            path.fill();
            match tone {
                Some(Tone::Neutral) | None => NSColor::separatorColor().setStroke(),
                Some(tone) => tone_color(*tone).setStroke(),
            }
            path.setLineWidth(1.0);
            path.stroke();
        }
        Shape::Rule => {
            NSColor::separatorColor().setFill();
            NSBezierPath::fillRect(rect(0.0, (h / 2.0).floor(), w, 1.0));
        }
        Shape::Bar(fraction, tone) => {
            let radius = h / 2.0;
            NSColor::quaternaryLabelColor().setFill();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                rect(0.0, 0.0, w, h),
                radius,
                radius,
            )
            .fill();
            match tone {
                Tone::Neutral => NSColor::secondaryLabelColor().setFill(),
                tone => tone_color(*tone).setFill(),
            }
            let filled = (w * fraction.clamp(0.0, 1.0)).max(if *fraction > 0.0 { h } else { 0.0 });
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                rect(0.0, 0.0, filled, h),
                radius,
                radius,
            )
            .fill();
        }
        Shape::Boxes(done, total) => {
            for i in 0..*total {
                let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                    rect(f64::from(i) * 24.0 + 0.5, 0.5, 19.0, h - 1.0),
                    2.0,
                    2.0,
                );
                if i < *done {
                    NSColor::systemGreenColor().setFill();
                    path.fill();
                } else {
                    NSColor::tertiaryLabelColor().setStroke();
                    path.stroke();
                }
            }
        }
        Shape::Chart(chart, compact) => draw_chart(chart, *compact, w, h),
    }
}

pub(super) fn draw_chart(chart: &Chart, compact: bool, w: f64, h: f64) {
    let (left, right, top, bottom) = if compact {
        (1.0, 3.0, 2.0, 2.0)
    } else {
        (28.0, 6.0, 6.0, 16.0)
    };
    let (pw, ph) = (w - left - right, h - top - bottom);
    let secondary = NSColor::secondaryLabelColor();
    if let Some(empty) = &chart.empty {
        if !compact {
            let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                rect(0.5, 0.5, w - 1.0, h - 1.0),
                6.0,
                6.0,
            );
            dashed(&path);
            NSColor::tertiaryLabelColor().setStroke();
            path.stroke();
            draw_text(empty, w / 2.0, h / 2.0 - 8.0, 11.0, &secondary, 1);
        }
        return;
    }
    let span = chart.t1.saturating_sub(chart.t0) as f64;
    let x = |t: u64| {
        if span <= 0.0 {
            left + pw / 2.0
        } else {
            left + (t.saturating_sub(chart.t0) as f64 / span) * pw
        }
    };
    let y = |v: f64| top + ph - (v / chart.y_max).clamp(0.0, 1.0) * ph;
    for gap in &chart.gaps {
        let (x0, x1) = (x(gap.from), x(gap.to));
        NSColor::quaternaryLabelColor()
            .colorWithAlphaComponent(0.35)
            .setFill();
        NSBezierPath::fillRect(rect(x0, top, (x1 - x0).max(1.0), ph));
        if !compact && x1 - x0 >= 26.0 {
            draw_text(gap.label, (x0 + x1) / 2.0, top + 2.0, 10.0, &secondary, 1);
        }
    }
    if !compact {
        for (value, label) in &chart.y_ticks {
            NSColor::separatorColor().setFill();
            NSBezierPath::fillRect(rect(left, y(*value).floor(), pw, 1.0));
            draw_text(label, left - 4.0, y(*value) - 7.0, 10.0, &secondary, 2);
        }
        for (i, (t, label)) in chart.x_labels.iter().enumerate() {
            let align = if i == 0 { 0 } else { 2 };
            draw_text(label, x(*t), h - 13.0, 10.0, &secondary, align);
        }
    }
    for (value, label) in &chart.thresholds {
        if compact || *value > chart.y_max {
            continue;
        }
        let path = NSBezierPath::bezierPath();
        path.moveToPoint(NSPoint::new(left, y(*value)));
        path.lineToPoint(NSPoint::new(left + pw, y(*value)));
        path.setLineWidth(1.0);
        dashed(&path);
        let color = if *value >= 16.0 {
            NSColor::systemRedColor()
        } else {
            NSColor::systemOrangeColor()
        };
        color.setStroke();
        path.stroke();
        if !compact {
            draw_text(label, left + pw - 2.0, y(*value) - 13.0, 10.0, &color, 2);
        }
    }
    let line = if chart.unit.starts_with("MiB/s") {
        NSColor::systemOrangeColor()
    } else {
        NSColor::systemBlueColor()
    };
    line.setStroke();
    line.setFill();
    for segment in &chart.segments {
        if segment.len() == 1 {
            let (t, v) = segment[0];
            NSBezierPath::bezierPathWithOvalInRect(rect(x(t) - 2.0, y(v) - 2.0, 4.0, 4.0)).fill();
            continue;
        }
        let path = NSBezierPath::bezierPath();
        for (i, (t, v)) in segment.iter().enumerate() {
            let point = NSPoint::new(x(*t), y(*v));
            if i == 0 {
                path.moveToPoint(point);
            } else {
                path.lineToPoint(point);
            }
        }
        path.setLineWidth(if compact { 1.2 } else { 1.6 });
        path.stroke();
    }
}
