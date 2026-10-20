//! Layout for each `panel::Block` kind, built from `Kit` pieces.

use super::*;

impl Kit<'_> {
    pub(super) fn block(&self, parent: &NSView, block: &Block, x: f64, y: f64, w: f64) -> f64 {
        let mtm = self.mtm;
        let label = NSColor::labelColor();
        let secondary = NSColor::secondaryLabelColor();
        let regular = unsafe { NSFontWeightRegular };
        let semibold = unsafe { NSFontWeightSemibold };
        match block {
            Block::Conclusion {
                tone,
                title,
                subtitle,
            } => {
                self.icon(parent, *tone, x, y + 2.0, 28.0);
                let tx = x + 40.0;
                let th = self.text(parent, title, 17.0, semibold, &label, tx, y, w - 40.0);
                let sh = self.text(
                    parent,
                    subtitle,
                    12.0,
                    regular,
                    &secondary,
                    tx,
                    y + th + 2.0,
                    w - 40.0,
                );
                (th + 2.0 + sh).max(32.0)
            }
            Block::Header { back, title, badge } => {
                let back = Button {
                    title: format!("‹ {back}"),
                    command: Command::Back,
                    primary: false,
                    enabled: true,
                };
                let control = self.button(&back);
                control.setBordered(false);
                control.setContentTintColor(Some(&NSColor::controlAccentColor()));
                control.sizeToFit();
                let bw = control.frame().size.width;
                self.put(parent, &control, x - 4.0, y, bw, 22.0);
                let mut badge_w = 0.0;
                if let Some((tone, text)) = badge {
                    let (field, h) = self.label(
                        text,
                        12.0,
                        unsafe { NSFontWeightMedium },
                        &tone_color(*tone),
                        140.0,
                        false,
                    );
                    if matches!(tone, Tone::Neutral | Tone::Unknown | Tone::Paused) {
                        field.setTextColor(Some(&secondary));
                    }
                    let fw = field
                        .sizeThatFits(NSSize::new(140.0, 30.0))
                        .width
                        .ceil()
                        .min(140.0);
                    self.put(parent, &field, x + w - fw, y + (22.0 - h) / 2.0, fw, h);
                    self.icon(parent, *tone, x + w - fw - 17.0, y + 4.0, 12.0);
                    badge_w = fw + 22.0;
                }
                let tx = x + bw + 4.0;
                let (field, h) =
                    self.label(title, 15.0, semibold, &label, w - bw - badge_w - 8.0, false);
                self.put(
                    parent,
                    &field,
                    tx,
                    y + (22.0 - h) / 2.0,
                    w - bw - badge_w - 8.0,
                    h,
                );
                22.0
            }
            Block::Card {
                tone,
                title,
                body,
                buttons,
            } => self.card(parent, Some(*tone), title, body, buttons, x, y, w),
            Block::Heading(text) => self.text(parent, text, 12.0, semibold, &secondary, x, y, w),
            Block::Text(text) => self.text(parent, text, 12.5, regular, &label, x, y, w),
            Block::Note(text) => self.text(parent, text, 11.0, regular, &secondary, x, y, w),
            Block::Big { value, unit } => {
                let (big, bh) = self.label(value, 26.0, semibold, &label, w, false);
                big.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(
                    26.0, semibold,
                )));
                let bw = big.sizeThatFits(NSSize::new(w, 60.0)).width.ceil();
                self.put(parent, &big, x, y, bw, bh);
                let (small, sh) = self.label(unit, 13.0, regular, &secondary, w - bw - 6.0, false);
                self.put(
                    parent,
                    &small,
                    x + bw + 6.0,
                    y + bh - sh - 4.0,
                    w - bw - 6.0,
                    sh,
                );
                bh
            }
            Block::Graphic(graphic) => match graphic {
                Graphic::Chart(chart) => self.chart_card(parent, chart, x, y, w),
                Graphic::Bar { label: text, .. } => {
                    let view = self.graphic_view(graphic, rect(x, y, w, 8.0));
                    parent.addSubview(&view);
                    let h = self.text(parent, text, 11.0, regular, &secondary, x, y + 12.0, w);
                    12.0 + h
                }
                Graphic::Spark(_) => {
                    let view = self.graphic_view(graphic, rect(x, y, 96.0, 24.0));
                    parent.addSubview(&view);
                    24.0
                }
            },
            Block::Facts(facts) => {
                let card = ShapeView::new(mtm, Shape::Card(None), rect(x, y, w, 10.0));
                let inner = w - 24.0;
                let mut cy = 4.0;
                for (i, (key, value)) in facts.iter().enumerate() {
                    if i > 0 {
                        self.put(
                            &card,
                            &ShapeView::new(mtm, Shape::Rule, rect(0.0, 0.0, 0.0, 0.0)),
                            12.0,
                            cy,
                            inner,
                            1.0,
                        );
                    }
                    cy += 6.0;
                    let (vfield, _) = self.label(
                        value,
                        12.0,
                        unsafe { NSFontWeightMedium },
                        &label,
                        inner,
                        false,
                    );
                    let vw = vfield
                        .sizeThatFits(NSSize::new(10_000.0, 30.0))
                        .width
                        .ceil();
                    if vw <= inner * 0.55 {
                        let (kfield, kh) =
                            self.label(key, 12.0, regular, &secondary, inner - vw - 8.0, false);
                        self.put(&card, &kfield, 12.0, cy, inner - vw - 8.0, kh);
                        vfield.setAlignment(NSTextAlignment::Right);
                        self.put(&card, &vfield, 12.0 + inner - vw, cy, vw, kh);
                        cy += kh;
                    } else {
                        cy += self.text(
                            &card,
                            key,
                            12.0,
                            unsafe { NSFontWeightMedium },
                            &label,
                            12.0,
                            cy,
                            inner,
                        );
                        cy += 2.0;
                        cy += self.text(&card, value, 12.0, regular, &secondary, 12.0, cy, inner);
                    }
                    cy += 6.0;
                }
                cy += 4.0;
                card.setFrameSize(NSSize::new(w, cy));
                parent.addSubview(&card);
                cy
            }
            Block::Steps(steps) | Block::Bullets(steps) => {
                let numbered = matches!(block, Block::Steps(_));
                let mut cy = 0.0;
                for (i, step) in steps.iter().enumerate() {
                    let mark = if numbered {
                        format!("{}.", i + 1)
                    } else {
                        "•".into()
                    };
                    let (field, _) = self.label(&mark, 12.5, semibold, &secondary, 18.0, false);
                    self.put(parent, &field, x, y + cy, 18.0, 17.0);
                    cy += self.text(
                        parent,
                        step,
                        12.5,
                        regular,
                        &label,
                        x + 20.0,
                        y + cy,
                        w - 20.0,
                    ) + 4.0;
                }
                cy - 4.0
            }
            Block::Buttons(buttons) => self.buttons(parent, buttons, x, y, w),
            Block::Choices(choices) => {
                let mut cy = 0.0;
                for choice in choices {
                    // SAFETY: `perform:` is defined on Controller, which lives for the whole app.
                    let radio = unsafe {
                        NSButton::radioButtonWithTitle_target_action(
                            &NSString::from_str(&choice.title),
                            Some(self.target),
                            Some(sel!(perform:)),
                            mtm,
                        )
                    };
                    radio.setTag(self.tag(&choice.command));
                    radio.setState(if choice.primary {
                        NSControlStateValueOn
                    } else {
                        NSControlStateValueOff
                    });
                    radio.sizeToFit();
                    let size = radio.frame().size;
                    self.put(parent, &radio, x, y + cy, size.width, size.height);
                    cy += size.height + 4.0;
                }
                cy - 4.0
            }
            Block::Progress {
                done,
                total,
                label: text,
            } => {
                let boxes = ShapeView::new(
                    mtm,
                    Shape::Boxes(*done, *total),
                    rect(x, y + 3.0, 24.0 * f64::from(*total), 10.0),
                );
                boxes.setAccessibilityElement(true);
                boxes.setAccessibilityLabel(Some(&NSString::from_str(text)));
                parent.addSubview(&boxes);
                let tx = x + 24.0 * f64::from(*total) + 6.0;
                self.text(parent, text, 12.0, regular, &secondary, tx, y, w - (tx - x))
                    .max(16.0)
            }
            Block::Processes(rows) => self.process_rows(parent, rows, x, y, w),
            Block::Resource { .. } => 0.0,
        }
    }

    pub(super) fn chart_card(&self, parent: &NSView, chart: &Chart, x: f64, y: f64, w: f64) -> f64 {
        let mtm = self.mtm;
        let card = ShapeView::new(mtm, Shape::Card(None), rect(x, y, w, 10.0));
        let inner = w - 24.0;
        let (title, th) = self.label(
            &chart.title,
            12.0,
            unsafe { NSFontWeightSemibold },
            &NSColor::labelColor(),
            inner / 2.0,
            false,
        );
        self.put(&card, &title, 12.0, 10.0, inner / 2.0, th);
        let (unit, uh) = self.label(
            &chart.unit,
            11.0,
            unsafe { NSFontWeightRegular },
            &NSColor::secondaryLabelColor(),
            inner / 2.0,
            false,
        );
        unit.setAlignment(NSTextAlignment::Right);
        self.put(
            &card,
            &unit,
            12.0 + inner / 2.0,
            10.0 + (th - uh),
            inner / 2.0,
            uh,
        );
        let mut cy = 10.0 + th + 6.0;
        let plot = self.graphic_view(&Graphic::Chart(chart.clone()), rect(12.0, cy, inner, 104.0));
        card.addSubview(&plot);
        cy += 104.0 + 4.0;
        if !chart.note.is_empty() {
            cy += self.text(
                &card,
                &chart.note,
                11.0,
                unsafe { NSFontWeightRegular },
                &NSColor::secondaryLabelColor(),
                12.0,
                cy,
                inner,
            );
        }
        cy += 10.0;
        card.setFrameSize(NSSize::new(w, cy));
        parent.addSubview(&card);
        cy
    }
}
