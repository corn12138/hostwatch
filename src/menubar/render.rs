//! Turns the current `panel::Page` into the popover view tree, keeping scroll position and
//! keyboard focus across re-renders of the same page.

use super::*;

impl Controller {
    pub(super) fn render(&self, force: bool) {
        if !force && !self.shown() {
            return;
        }
        let (Some(root), Some(scroll), Some(popover)) = (
            self.ivars().root.get(),
            self.ivars().scroll.get(),
            self.ivars().popover.get(),
        ) else {
            return;
        };
        autoreleasepool(|_| {
            let mtm = self.mtm();
            let now = now_unix();
            let (route, back) = {
                let routes = self.ivars().routes.borrow();
                let route = *routes.last().unwrap_or(&Route::Home);
                let back = routes.len().checked_sub(2).map(|i| routes[i]);
                (route, back)
            };
            let (page, health) = {
                let monitor = self.ivars().monitor.borrow();
                (panel::page(&monitor, route, back, now), monitor.health(now))
            };
            let focused = self.focused_command(root);
            let same_route = self.ivars().rendered.get().map(|r| r.1) == Some(route);
            let offset = scroll.contentView().bounds().origin;
            let kit = Kit::new(mtm, self);

            let doc = Plain::new(mtm, rect(0.0, 0.0, WIDTH, 10.0));
            let mut y = 14.0;
            let mut blocks = page.blocks.iter().peekable();
            while let Some(block) = blocks.next() {
                let height = if let Block::Resource { .. } = block {
                    let mut rows = vec![block];
                    while let Some(next @ Block::Resource { .. }) = blocks.peek() {
                        rows.push(next);
                        blocks.next();
                    }
                    kit.resource_group(&doc, &rows, y)
                } else {
                    kit.block(&doc, block, PAD, y, WIDTH - 2.0 * PAD)
                };
                y += height + GAP;
            }
            let content = y - GAP + 14.0;
            doc.setFrameSize(NSSize::new(WIDTH, content));

            let screen = NSScreen::mainScreen(mtm).map_or(800.0, |s| s.visibleFrame().size.height);
            let visible = content.min((screen - 40.0 - FOOTER).max(240.0));
            scroll.setFrame(rect(0.0, 0.0, WIDTH, visible));
            scroll.setDocumentView(Some(&doc));
            if same_route {
                doc.scrollPoint(offset);
            } else {
                doc.scrollPoint(NSPoint::new(0.0, 0.0));
            }
            for view in root.subviews().iter() {
                if &*view != scroll.as_ref() as &NSView {
                    view.removeFromSuperview();
                }
            }
            let footer = kit.footer(&page.footer, visible);
            root.addSubview(&footer);
            *self.ivars().commands.borrow_mut() = kit.commands.into_inner();
            *self.ivars().more_button.borrow_mut() = kit.more.into_inner();
            let size = NSSize::new(WIDTH, visible + FOOTER);
            root.setFrameSize(size);
            popover.setContentSize(size);
            self.restore_focus(root, focused);
            self.ivars().rendered.set(Some((health, route)));
        });
    }

    pub(super) fn focused_command(&self, root: &RootView) -> Option<Command> {
        let responder = root.window()?.firstResponder()?;
        let control = responder.downcast_ref::<NSControl>()?;
        let index = usize::try_from(control.tag() - 1).ok()?;
        self.ivars().commands.borrow().get(index).cloned()
    }

    pub(super) fn restore_focus(&self, root: &RootView, command: Option<Command>) {
        let Some(window) = root.window() else {
            return;
        };
        let index =
            command.and_then(|c| self.ivars().commands.borrow().iter().position(|x| *x == c));
        match index.and_then(|i| root.viewWithTag(i as isize + 1)) {
            Some(view) => {
                window.makeFirstResponder(Some(&view));
            }
            None => {
                if window.firstResponder().is_none() {
                    window.makeFirstResponder(Some(root));
                }
            }
        }
    }
}
