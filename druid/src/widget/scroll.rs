// Copyright 2019 The Druid Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A container that scrolls its contents.

use crate::commands::SCROLL_TO_VIEW;
use crate::contexts::ChangeCtx;
use crate::debug_state::DebugState;
use crate::widget::prelude::*;
use crate::widget::{Axis, ClipBox};
use crate::{scroll_component::*, Data, Rect, Vec2};
use tracing::{instrument, trace};

/// A container that scrolls its contents.
///
/// This container holds a single child, and uses the wheel to scroll it
/// when the child's bounds are larger than the viewport.
///
/// The child is laid out with completely unconstrained layout bounds by
/// default. Restrict to a specific axis with [`vertical`] or [`horizontal`].
/// When restricted to scrolling on a specific axis the child's size is
/// locked on the opposite axis.
///
/// [`vertical`]: struct.Scroll.html#method.vertical
/// [`horizontal`]: struct.Scroll.html#method.horizontal
pub struct Scroll<T, W> {
    clip: ClipBox<T, W>,
    scroll_component: ScrollComponent,
    scroll_snap_vertical: Box<dyn Fn(&T, &Env) -> bool>,
    scroll_snap_horizontal: Box<dyn Fn(&T, &Env) -> bool>,
}

impl<T, W: Widget<T>> Scroll<T, W> {
    /// Create a new scroll container.
    ///
    /// This method will allow scrolling in all directions if child's bounds
    /// are larger than the viewport. Use [vertical](#method.vertical) and
    /// [horizontal](#method.horizontal) methods to limit scrolling to a specific axis.
    pub fn new(child: W) -> Scroll<T, W> {
        Scroll {
            clip: ClipBox::managed(child),
            scroll_component: ScrollComponent::new(),
            scroll_snap_vertical: Box::new(|_, _| false),
            scroll_snap_horizontal: Box::new(|_, _| false),
        }
    }

    /// Scroll by `delta` units.
    ///
    /// Returns `true` if the scroll offset has changed.
    pub fn scroll_by<C: ChangeCtx>(&mut self, ctx: &mut C, delta: Vec2) -> bool {
        self.clip.pan_by(ctx, delta)
    }

    /// Scroll the minimal distance to show the target `region`.
    ///
    /// If the target region is larger than the viewport, we will display the
    /// portion that fits, prioritizing the portion closest to the origin.
    pub fn scroll_to<C: ChangeCtx>(&mut self, ctx: &mut C, region: Rect) -> bool {
        self.clip.pan_to_visible(ctx, region)
    }

    /// Scroll to this position on a particular axis.
    ///
    /// Returns `true` if the scroll offset has changed.
    pub fn scroll_to_on_axis<C: ChangeCtx>(
        &mut self,
        ctx: &mut C,
        axis: Axis,
        position: f64,
    ) -> bool {
        self.clip.pan_to_on_axis(ctx, axis, position)
    }
}

impl<T, W> Scroll<T, W> {
    /// Restrict scrolling to the vertical axis while locking child width.
    pub fn vertical(mut self) -> Self {
        self.scroll_component.enabled = ScrollbarsEnabled::Vertical;
        self.clip.set_constrain_vertical(false);
        self.clip.set_constrain_horizontal(true);
        self
    }

    /// Restrict scrolling to the horizontal axis while locking child height.
    pub fn horizontal(mut self) -> Self {
        self.scroll_component.enabled = ScrollbarsEnabled::Horizontal;
        self.clip.set_constrain_vertical(true);
        self.clip.set_constrain_horizontal(false);
        self
    }

    /// Builder-style method to set whether the child must fill the view.
    ///
    /// If `false` (the default) there is no minimum constraint on the child's
    /// size. If `true`, the child must have at least the same size as the parent
    /// `Scroll` widget.
    pub fn content_must_fill(mut self, must_fill: bool) -> Self {
        self.set_content_must_fill(must_fill);
        self
    }

    /// Whether the view should snap vertically when the child size changes.
    /// If `false` (the default) the vertical view will remain stationary
    /// regardless of new data.
    /// If `true`, whenever the child size changes, the view will snap to the
    /// bottom.
    pub fn with_snap_vertical(mut self, snap: impl Fn(&T, &Env) -> bool + 'static) -> Self {
        self.scroll_snap_vertical = Box::new(snap);
        self
    }

    /// Whether the view should snap horizontally when the child size changes.
    /// If `false` (the default) the horizontal view will remain stationary
    /// regardless of new data.
    /// If `true`, whenever the child size changes, the view will snap to the
    /// far right.
    pub fn with_snap_horizontal(mut self, snap: impl Fn(&T, &Env) -> bool + 'static) -> Self {
        self.scroll_snap_horizontal = Box::new(snap);
        self
    }

    /// Disable both scrollbars
    pub fn disable_scrollbars(mut self) -> Self {
        self.scroll_component.enabled = ScrollbarsEnabled::None;
        self
    }

    /// Set whether the child's size must be greater than or equal the size of
    /// the `Scroll` widget.
    ///
    /// See [`content_must_fill`] for more details.
    ///
    /// [`content_must_fill`]: Scroll::content_must_fill
    pub fn set_content_must_fill(&mut self, must_fill: bool) {
        self.clip.set_content_must_fill(must_fill);
    }

    /// Set which scrollbars should be enabled.
    ///
    /// If scrollbars are disabled, scrolling will still occur as a result of
    /// scroll events from a trackpad or scroll wheel.
    pub fn set_enabled_scrollbars(&mut self, enabled: ScrollbarsEnabled) {
        self.scroll_component.enabled = enabled;
    }

    /// Set whether the content can be scrolled in the vertical direction.
    pub fn set_vertical_scroll_enabled(&mut self, enabled: bool) {
        self.clip.set_constrain_vertical(!enabled);
        self.scroll_component
            .enabled
            .set_vertical_scrollbar_enabled(enabled);
    }

    /// Set whether the content can be scrolled in the horizontal direction.
    pub fn set_horizontal_scroll_enabled(&mut self, enabled: bool) {
        self.clip.set_constrain_horizontal(!enabled);
        self.scroll_component
            .enabled
            .set_horizontal_scrollbar_enabled(enabled);
    }

    /// Returns a reference to the child widget.
    pub fn child(&self) -> &W {
        self.clip.child()
    }

    /// Returns a mutable reference to the child widget.
    pub fn child_mut(&mut self) -> &mut W {
        self.clip.child_mut()
    }

    /// Returns the size of the child widget.
    pub fn child_size(&self) -> Size {
        self.clip.content_size()
    }

    /// Returns the current scroll offset.
    pub fn offset(&self) -> Vec2 {
        self.clip.viewport_origin().to_vec2()
    }

    /// Returns a [`Rect`] representing the currently visible region.
    ///
    /// This is relative to the bounds of the content.
    pub fn viewport_rect(&self) -> Rect {
        self.clip.viewport().view_rect()
    }

    /// Return the scroll offset on a particular axis
    pub fn offset_for_axis(&self, axis: Axis) -> f64 {
        axis.major_pos(self.clip.viewport_origin())
    }
}

impl<T: Data, W: Widget<T>> Widget<T> for Scroll<T, W> {
    #[instrument(name = "Scroll", level = "trace", skip(self, ctx, event, data, env))]
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        let scroll_component = &mut self.scroll_component;
        self.clip.with_port(ctx, |ctx, port| {
            scroll_component.event(port, ctx, event, env);
        });
        if !ctx.is_handled() {
            self.clip.event(ctx, event, data, env);
        }

        // Handle scroll after the inner widget processed the events, to prefer inner widgets while
        // scrolling.
        self.clip.with_port(ctx, |ctx, port| {
            scroll_component.handle_scroll(port, ctx, event, env);

            if !scroll_component.are_bars_held() {
                // We only scroll to the component if the user is not trying to move the scrollbar.
                if let Event::Notification(notification) = event {
                    if let Some(&global_highlight_rect) = notification.get(SCROLL_TO_VIEW) {
                        ctx.set_handled();
                        let view_port_changed =
                            port.default_scroll_to_view_handling(ctx, global_highlight_rect);
                        if view_port_changed {
                            scroll_component
                                .reset_scrollbar_fade(|duration| ctx.request_timer(duration), env);
                        }
                    }
                }
            }
        });
    }

    #[instrument(name = "Scroll", level = "trace", skip(self, ctx, event, data, env))]
    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, data: &T, env: &Env) {
        self.scroll_component.lifecycle(ctx, event, env);
        self.clip.lifecycle(ctx, event, data, env);
    }

    #[instrument(name = "Scroll", level = "trace", skip(self, ctx, old_data, data, env))]
    fn update(&mut self, ctx: &mut UpdateCtx, old_data: &T, data: &T, env: &Env) {
        self.clip.update(ctx, old_data, data, env);
    }

    #[instrument(name = "Scroll", level = "trace", skip(self, ctx, bc, data, env))]
    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, data: &T, env: &Env) -> Size {
        bc.debug_check("Scroll");

        let old_content_size = self.clip.viewport().content_size;
        let old_self_size = self.clip.viewport().view_size;

        let child_size = self.clip.layout(ctx, bc, data, env);
        log_size_warnings(child_size);

        let self_size = bc.constrain(child_size);
        // The new size might have made the current scroll offset invalid. This makes it valid
        // again.
        let _ = self.scroll_by(ctx, Vec2::ZERO);
        if old_self_size != self_size {
            self.scroll_component
                .reset_scrollbar_fade(|d| ctx.request_timer(d), env);
        }

        if ((self.scroll_snap_vertical)(data, env) || (self.scroll_snap_horizontal)(data, env))
            && self.clip.viewport().content_size != old_content_size
        {
            let viewport = self.clip.viewport();
            let mut distance = Vec2::ZERO;
            if (self.scroll_snap_vertical)(data, env) {
                distance.y = viewport.content_size.height
                    - (viewport.view_origin.y + viewport.view_size.height);
            }
            if (self.scroll_snap_horizontal)(data, env) {
                distance.x = viewport.content_size.width
                    - (viewport.view_origin.x + viewport.view_size.width);
            }
            self.scroll_by(ctx, distance);
        }

        trace!("Computed size: {}", self_size);
        self_size
    }

    #[instrument(name = "Scroll", level = "trace", skip(self, ctx, data, env))]
    fn paint(&mut self, ctx: &mut PaintCtx, data: &T, env: &Env) {
        self.clip.paint(ctx, data, env);
        self.scroll_component
            .draw_bars(ctx, &self.clip.viewport(), env);
    }

    fn debug_state(&self, data: &T) -> DebugState {
        DebugState {
            display_name: self.short_type_name().to_string(),
            children: vec![self.clip.debug_state(data)],
            ..Default::default()
        }
    }
}

fn log_size_warnings(size: Size) {
    if size.width.is_infinite() {
        tracing::warn!("Scroll widget's child has an infinite width.");
    }

    if size.height.is_infinite() {
        tracing::warn!("Scroll widget's child has an infinite height.");
    }
}
