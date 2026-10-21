//! Grouped rows built from `Kit` pieces: resource rows on the home page, process rows and the
//! fixed footer.

use super::*;

impl Kit<'_> {
    pub(super) fn resource_group(&self, parent: &NSView, rows: &[&Block], y: f64) -> f64 {
        const ROW: f64 = 52.0;
        let mtm = self.mtm;
        let w = WIDTH - 2.0 * PAD;
        let card = ShapeView::new(
            mtm,
            Shape::Card(None),
            rect(PAD, y, w, ROW * rows.len() as f64),
        );
        // The name column fits the widest name, so longer English names are not truncated.
        let name_column = rows
            .iter()
            .filter_map(|row| match row {
                Block::Resource { name, .. } => Some(name),
                _ => None,
            })
            .map(|name| {
                let (field, _) = self.label(
                    name,
                    13.0,
                    unsafe { NSFontWeightSemibold },
                    &NSColor::labelColor(),
                    200.0,
                    false,
                );
                field.sizeThatFits(NSSize::new(200.0, 40.0)).width.ceil()
            })
            .fold(60.0f64, f64::max)
            .min(120.0);
        let value_x = 12.0 + name_column + 12.0;
        for (i, row) in rows.iter().enumerate() {
            let Block::Resource {
                resource,
                tone,
                name,
                value,
                status,
                graphic,
                dim,
                label,
            } = row
            else {
                continue;
            };
            let top = ROW * i as f64;
            if i > 0 {
                self.put(
                    &card,
                    &ShapeView::new(mtm, Shape::Rule, rect(0.0, 0.0, 0.0, 0.0)),
                    12.0,
                    top - 0.5,
                    w - 24.0,
                    1.0,
                );
            }
            let content = Plain::new(mtm, rect(0.0, top, w, ROW));
            let primary = NSColor::labelColor();
            let secondary = NSColor::secondaryLabelColor();
            let (name_field, nh) = self.label(
                name,
                13.0,
                unsafe { NSFontWeightSemibold },
                &primary,
                name_column,
                false,
            );
            self.put(
                &content,
                &name_field,
                12.0,
                (ROW - nh) / 2.0,
                name_column,
                nh,
            );
            let (value_field, vh) = self.label(
                value,
                15.0,
                unsafe { NSFontWeightSemibold },
                &primary,
                150.0,
                false,
            );
            value_field.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(
                15.0,
                unsafe { NSFontWeightSemibold },
            )));
            let value_width = value_field
                .sizeThatFits(NSSize::new(150.0, 40.0))
                .width
                .ceil()
                .min(150.0);
            self.put(
                &content,
                &value_field,
                value_x,
                (ROW - vh) / 2.0,
                value_width,
                vh,
            );
            let gx = value_x + value_width + 10.0;
            let (status_field, sh) = self.label(
                status,
                12.0,
                unsafe { NSFontWeightMedium },
                &tone_color(*tone),
                150.0,
                false,
            );
            if matches!(tone, Tone::Neutral | Tone::Unknown | Tone::Paused) {
                status_field.setTextColor(Some(&secondary));
            }
            status_field.setAlignment(NSTextAlignment::Right);
            let sw = status_field
                .sizeThatFits(NSSize::new(150.0, 40.0))
                .width
                .ceil()
                .min(150.0);
            let sx = w - 24.0 - sw;
            self.put(&content, &status_field, sx, (ROW - sh) / 2.0, sw, sh);
            self.icon(&content, *tone, sx - 17.0, (ROW - 14.0) / 2.0, 12.0);
            // Trends and bars use only the room left before the status, and are skipped without it.
            let room = sx - 17.0 - 10.0 - gx;
            let small = match graphic {
                Some(g @ Graphic::Spark(_)) => Some((g, room.min(96.0), 24.0)),
                Some(g @ Graphic::Bar { .. }) => Some((g, room.min(80.0), 6.0)),
                _ => None,
            };
            if let Some((g, width, height)) = small.filter(|s| s.1 >= 32.0) {
                let view = self.graphic_view(g, rect(gx, (ROW - height) / 2.0, width, height));
                view.setAccessibilityElement(false);
                content.addSubview(&view);
            }
            let (chevron, ch) = self.label(
                "›",
                15.0,
                unsafe { NSFontWeightRegular },
                &secondary,
                12.0,
                false,
            );
            self.put(&content, &chevron, w - 18.0, (ROW - ch) / 2.0, 10.0, ch);
            if *dim {
                content.setAlphaValue(0.5);
            }
            card.addSubview(&content);
            let route = Command::Go(Route::Detail(*resource));
            // SAFETY: `perform:` is defined on Controller, which lives for the whole app.
            let overlay = unsafe {
                NSButton::buttonWithTitle_target_action(
                    ns_string!(""),
                    Some(self.target),
                    Some(sel!(perform:)),
                    mtm,
                )
            };
            overlay.setBordered(false);
            overlay.setTag(self.tag(&route));
            overlay.setAccessibilityLabel(Some(&NSString::from_str(label)));
            self.put(&card, &overlay, 0.0, top, w, ROW);
        }
        parent.addSubview(&card);
        ROW * rows.len() as f64
    }

    pub(super) fn process_rows(
        &self,
        parent: &NSView,
        rows: &[ProcessRow],
        x: f64,
        y: f64,
        w: f64,
    ) -> f64 {
        const ROW: f64 = 30.0;
        let mtm = self.mtm;
        let card = ShapeView::new(
            mtm,
            Shape::Card(None),
            rect(x, y, w, ROW * rows.len() as f64),
        );
        let label = NSColor::labelColor();
        let secondary = NSColor::secondaryLabelColor();
        for (i, row) in rows.iter().enumerate() {
            let top = ROW * i as f64;
            if i > 0 {
                self.put(
                    &card,
                    &ShapeView::new(mtm, Shape::Rule, rect(0.0, 0.0, 0.0, 0.0)),
                    12.0,
                    top,
                    w - 24.0,
                    1.0,
                );
            }
            let (rank, h) = self.label(
                &row.rank.to_string(),
                12.0,
                unsafe { NSFontWeightRegular },
                &secondary,
                16.0,
                false,
            );
            self.put(&card, &rank, 12.0, top + (ROW - h) / 2.0, 16.0, h);
            let name_w = w - 32.0 - 150.0;
            let (name, h) = self.label(
                &row.name,
                12.5,
                unsafe { NSFontWeightRegular },
                &label,
                name_w,
                false,
            );
            self.put(&card, &name, 32.0, top + (ROW - h) / 2.0, name_w, h);
            let bar = ShapeView::new(
                mtm,
                Shape::Bar(row.fraction, Tone::Neutral),
                rect(w - 150.0, top + (ROW - 4.0) / 2.0, 64.0, 4.0),
            );
            card.addSubview(&bar);
            let (size, h) = self.label(
                &row.size,
                12.0,
                unsafe { NSFontWeightSemibold },
                &label,
                70.0,
                false,
            );
            size.setAlignment(NSTextAlignment::Right);
            self.put(&card, &size, w - 82.0, top + (ROW - h) / 2.0, 70.0, h);
        }
        parent.addSubview(&card);
        ROW * rows.len() as f64
    }

    pub(super) fn footer(&self, footer: &Footer, top: f64) -> Retained<Plain> {
        let mtm = self.mtm;
        let view = Plain::new(mtm, rect(0.0, top, WIDTH, FOOTER));
        self.put(
            &view,
            &ShapeView::new(mtm, Shape::Rule, rect(0.0, 0.0, 0.0, 0.0)),
            0.0,
            0.0,
            WIDTH,
            1.0,
        );
        let small = |control: &NSButton| {
            control.setBordered(false);
            control.setFont(Some(&NSFont::systemFontOfSize(12.0)));
            control.sizeToFit();
        };
        let pause = self.button(&footer.pause);
        let paused = footer.paused;
        if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(if paused { "play.fill" } else { "pause.fill" }),
            None,
        ) {
            pause.setImage(Some(&image));
            pause.setImagePosition(NSCellImagePosition::ImageLeading);
        }
        small(&pause);
        let pw = pause.frame().size.width;
        self.put(&view, &pause, PAD - 4.0, (FOOTER - 20.0) / 2.0, pw, 20.0);
        let interval = self.button(&footer.interval);
        small(&interval);
        interval.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
        let iw = interval.frame().size.width;
        self.put(
            &view,
            &interval,
            PAD + pw + 8.0,
            (FOOTER - 20.0) / 2.0,
            iw,
            20.0,
        );
        let more = self.button(&Button {
            title: tr!(self.lang, "更多", "More").into(),
            command: Command::More,
            primary: false,
            enabled: true,
        });
        if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            ns_string!("chevron.down"),
            None,
        ) {
            more.setImage(Some(&image));
            more.setImagePosition(NSCellImagePosition::ImageTrailing);
        }
        small(&more);
        let mw = more.frame().size.width;
        self.put(
            &view,
            &more,
            WIDTH - PAD - mw + 4.0,
            (FOOTER - 20.0) / 2.0,
            mw,
            20.0,
        );
        *self.more.borrow_mut() = Some(more);
        view
    }
}
