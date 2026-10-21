//! A small reusable AppKit view kit: labels, icons, buttons and cards with manual layout.

use super::*;

/// Builds views for one render pass and records the command behind each control: a control's
/// tag is its command's index + 1.
pub(super) struct Kit<'a> {
    pub(super) mtm: MainThreadMarker,
    pub(super) lang: Lang,
    pub(super) target: &'a AnyObject,
    pub(super) commands: RefCell<Vec<Command>>,
    pub(super) more: RefCell<Option<Retained<NSButton>>>,
}

impl<'a> Kit<'a> {
    /// `target` receives `perform:` from every control the kit creates.
    pub(super) fn new(mtm: MainThreadMarker, lang: Lang, target: &'a AnyObject) -> Self {
        Self {
            mtm,
            lang,
            target,
            commands: RefCell::new(Vec::new()),
            more: RefCell::new(None),
        }
    }

    pub(super) fn tag(&self, command: &Command) -> isize {
        let mut commands = self.commands.borrow_mut();
        commands.push(command.clone());
        commands.len() as isize
    }

    pub(super) fn label(
        &self,
        text: &str,
        size: f64,
        weight: NSFontWeight,
        color: &NSColor,
        width: f64,
        wrap: bool,
    ) -> (Retained<NSTextField>, f64) {
        let mtm = self.mtm;
        let string = NSString::from_str(text);
        let field = if wrap {
            NSTextField::wrappingLabelWithString(&string, mtm)
        } else {
            NSTextField::labelWithString(&string, mtm)
        };
        field.setFont(Some(&NSFont::systemFontOfSize_weight(size, weight)));
        field.setTextColor(Some(color));
        field.setSelectable(false);
        if wrap {
            field.setPreferredMaxLayoutWidth(width);
        } else {
            field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        }
        let height = field
            .sizeThatFits(NSSize::new(width, 10_000.0))
            .height
            .ceil();
        (field, height)
    }

    pub(super) fn put(&self, parent: &NSView, view: &NSView, x: f64, y: f64, w: f64, h: f64) {
        view.setFrame(rect(x, y, w, h));
        parent.addSubview(view);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn text(
        &self,
        parent: &NSView,
        text: &str,
        size: f64,
        weight: NSFontWeight,
        color: &NSColor,
        x: f64,
        y: f64,
        w: f64,
    ) -> f64 {
        let (field, h) = self.label(text, size, weight, color, w, true);
        self.put(parent, &field, x, y, w, h);
        h
    }

    pub(super) fn icon(&self, parent: &NSView, tone: Tone, x: f64, y: f64, size: f64) {
        let mtm = self.mtm;
        let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol(tone)),
            Some(&NSString::from_str(tone.label(self.lang))),
        ) else {
            return;
        };
        let configuration =
            NSImageSymbolConfiguration::configurationWithPointSize_weight(size, unsafe {
                NSFontWeightRegular
            });
        let view = NSImageView::imageViewWithImage(&image, mtm);
        view.setSymbolConfiguration(Some(&configuration));
        view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        view.setContentTintColor(Some(&tone_color(tone)));
        self.put(parent, &view, x, y, size + 2.0, size + 2.0);
    }

    pub(super) fn button(&self, button: &Button) -> Retained<NSButton> {
        let mtm = self.mtm;
        // SAFETY: `perform:` is defined on Controller, which lives for the whole app.
        let control = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(&button.title),
                Some(self.target),
                Some(sel!(perform:)),
                mtm,
            )
        };
        control.setTag(self.tag(&button.command));
        control.setEnabled(button.enabled);
        if button.primary {
            control.setBezelColor(Some(&NSColor::controlAccentColor()));
        }
        control.sizeToFit();
        control
    }

    /// Lays buttons out left to right, wrapping when they do not fit.
    pub(super) fn buttons(
        &self,
        parent: &NSView,
        buttons: &[Button],
        x: f64,
        y: f64,
        w: f64,
    ) -> f64 {
        let (mut cx, mut cy, mut row) = (x, y, 0.0f64);
        for button in buttons {
            let control = self.button(button);
            let size = control.frame().size;
            if cx > x && cx + size.width > x + w {
                cx = x;
                cy += row + 6.0;
                row = 0.0;
            }
            self.put(parent, &control, cx, cy, size.width.min(w), size.height);
            cx += size.width + 8.0;
            row = row.max(size.height);
        }
        cy + row - y
    }

    pub(super) fn graphic_view(&self, graphic: &Graphic, frame: NSRect) -> Retained<ShapeView> {
        let mtm = self.mtm;
        let (shape, label) = match graphic {
            Graphic::Chart(chart) => (
                Shape::Chart(Box::new(chart.clone()), false),
                chart.summary.clone(),
            ),
            Graphic::Spark(chart) => (
                Shape::Chart(Box::new(chart.clone()), true),
                chart.summary.clone(),
            ),
            Graphic::Bar {
                fraction,
                tone,
                label,
            } => (Shape::Bar(*fraction, *tone), label.clone()),
        };
        let view = ShapeView::new(mtm, shape, frame);
        view.setAccessibilityElement(true);
        // SAFETY: static AppKit accessibility role constant.
        view.setAccessibilityRole(Some(unsafe { NSAccessibilityImageRole }));
        view.setAccessibilityLabel(Some(&NSString::from_str(&label)));
        view
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn card(
        &self,
        parent: &NSView,
        tone: Option<Tone>,
        title: &str,
        body: &[String],
        buttons: &[Button],
        x: f64,
        y: f64,
        w: f64,
    ) -> f64 {
        let mtm = self.mtm;
        let card = ShapeView::new(mtm, Shape::Card(tone), rect(x, y, w, 10.0));
        let inner = w - 24.0;
        let mut cy = 12.0;
        let tx = if let Some(tone) = tone.filter(|t| *t != Tone::Neutral) {
            self.icon(&card, tone, 12.0, cy, 14.0);
            32.0
        } else {
            12.0
        };
        let label = NSColor::labelColor();
        cy += self.text(
            &card,
            title,
            14.0,
            unsafe { NSFontWeightSemibold },
            &label,
            tx,
            cy,
            inner - (tx - 12.0),
        );
        for line in body {
            cy += 4.0;
            cy += self.text(
                &card,
                line,
                12.0,
                unsafe { NSFontWeightRegular },
                &NSColor::secondaryLabelColor(),
                12.0,
                cy,
                inner,
            );
        }
        if !buttons.is_empty() {
            cy += 10.0;
            cy += self.buttons(&card, buttons, 12.0, cy, inner);
        }
        cy += 12.0;
        card.setFrameSize(NSSize::new(w, cy));
        parent.addSubview(&card);
        cy
    }
}
