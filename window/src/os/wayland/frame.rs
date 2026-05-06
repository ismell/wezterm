//! This file is derived from the ConceptFrame implementation
//! in smithay_client_toolkit 0.11 which is Copyright (c) 2018 Victor Berger
//! and provided under the terms of the MIT license.

//! The shiny new wezterm frame.

use std::error::Error;
use std::mem;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use smithay_client_toolkit::reexports::client::protocol::wl_shm;
use smithay_client_toolkit::reexports::client::protocol::wl_subsurface::WlSubsurface;
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{Dispatch, Proxy, QueueHandle};
use smithay_client_toolkit::reexports::csd_frame::{
    DecorationsFrame, FrameAction, FrameClick, ResizeEdge, WindowManagerCapabilities, WindowState,
};

use smithay_client_toolkit::compositor::SurfaceData;
use smithay_client_toolkit::seat::pointer::CursorIcon;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::Shm;
use smithay_client_toolkit::subcompositor::{SubcompositorState, SubsurfaceData};

use wayland_backend::client::ObjectId;

/// The size of the header bar.
const HEADER_SIZE: u32 = 0;

/// The size of the border.
const BORDER_SIZE: u32 = 4;

/// The size of the corner hit area.
const CORNER_SIZE: u32 = 16;

const PRIMARY_COLOR_ACTIVE: u32 = 0xFF3A3A3A;
const PRIMARY_COLOR_INACTIVE: u32 = 0xFF242424;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum LayoutState {
    Normal,
    Maximized,
}

#[derive(Debug)]
enum WeztermFrameState {
    Hidden,
    Visible {
        layout: LayoutState,
        active: bool,
        render_data: FrameRenderData,
    },
    Fullscreen,
}

/// The shiny new wezterm frame.
#[derive(Debug)]
pub struct WeztermFrame<State> {
    /// The parent surface.
    parent: WlSurface,

    /// The frame state.
    state: WeztermFrameState,

    /// The wm capabilities.
    wm_capabilities: WindowManagerCapabilities,

    /// Whether the frame is resizable.
    resizable: bool,

    /// Whether the frame is waiting for redraw.
    dirty: bool,

    /// The location of the mouse.
    mouse_location: Location,

    /// The location of the mouse.
    mouse_coords: (f64, f64),

    /// Whether the frame should sync with the parent.
    ///
    /// This should happen in reaction to scale or resize changes.
    should_sync: bool,

    /// The active scale factor of the frame.
    scale_factor: f64,

    /// The frame queue handle.
    queue_handle: QueueHandle<State>,

    /// The memory pool to use for drawing.
    pool: SlotPool,

    /// The subcompositor.
    subcompositor: Arc<SubcompositorState>,
}

impl<State> WeztermFrame<State>
where
    State: Dispatch<WlSurface, SurfaceData> + Dispatch<WlSubsurface, SubsurfaceData> + 'static,
{
    pub fn new(
        parent: &impl WaylandSurface,
        shm: &Shm,
        subcompositor: Arc<SubcompositorState>,
        queue_handle: QueueHandle<State>,
    ) -> Result<Self, Box<dyn Error>> {
        let parent = parent.wl_surface().clone();
        let pool = SlotPool::new(1, shm)?;
        let render_data = FrameRenderData::new(&parent, &subcompositor, &queue_handle);

        let wm_capabilities = WindowManagerCapabilities::all();
        Ok(Self {
            parent,
            resizable: true,
            state: WeztermFrameState::Visible {
                layout: LayoutState::Normal,
                active: false,
                render_data,
            },
            wm_capabilities,
            dirty: true,
            scale_factor: 1.,
            pool,
            should_sync: true,
            queue_handle,
            subcompositor,
            mouse_location: Location::None,
            mouse_coords: (0.0, 0.0),
        })
    }

    #[inline]
    fn part_for_surface(&self, surface_id: &ObjectId) -> Option<&FramePart> {
        match &self.state {
            WeztermFrameState::Visible { render_data, .. } => render_data.find_part(surface_id),
            _ => None,
        }
    }
}

impl<State> DecorationsFrame for WeztermFrame<State>
where
    State: Dispatch<WlSurface, SurfaceData> + Dispatch<WlSubsurface, SubsurfaceData> + 'static,
{
    fn set_scaling_factor(&mut self, scale_factor: f64) {
        self.scale_factor = scale_factor;
        self.dirty = true;
        self.should_sync = true;
    }

    fn on_click(
        &mut self,
        _timestamp: Duration,
        click: FrameClick,
        pressed: bool,
    ) -> Option<FrameAction> {
        // Handle alternate click before everything else.
        if click == FrameClick::Alternate {
            return if Location::Head != self.mouse_location
                || !self
                    .wm_capabilities
                    .contains(WindowManagerCapabilities::WINDOW_MENU)
            {
                None
            } else {
                Some(FrameAction::ShowMenu(
                    self.mouse_coords.0 as i32,
                    self.mouse_coords.1 as i32 - HEADER_SIZE as i32,
                ))
            };
        }

        let resize = pressed && self.resizable;
        match self.mouse_location {
            Location::Head if pressed => Some(FrameAction::Move),
            Location::Top if resize => Some(FrameAction::Resize(ResizeEdge::Top)),
            Location::TopLeft if resize => Some(FrameAction::Resize(ResizeEdge::TopLeft)),
            Location::Left if resize => Some(FrameAction::Resize(ResizeEdge::Left)),
            Location::BottomLeft if resize => Some(FrameAction::Resize(ResizeEdge::BottomLeft)),
            Location::Bottom if resize => Some(FrameAction::Resize(ResizeEdge::Bottom)),
            Location::BottomRight if resize => Some(FrameAction::Resize(ResizeEdge::BottomRight)),
            Location::Right if resize => Some(FrameAction::Resize(ResizeEdge::Right)),
            Location::TopRight if resize => Some(FrameAction::Resize(ResizeEdge::TopRight)),
            _ => None,
        }
    }

    fn click_point_moved(
        &mut self,
        _timestamp: Duration,
        surface_id: &ObjectId,
        x: f64,
        y: f64,
    ) -> Option<CursorIcon> {
        let location = self.part_for_surface(surface_id)?.precise_location(x);
        self.mouse_coords = (x, y);
        self.mouse_location = if matches!(
            self.state,
            WeztermFrameState::Visible {
                layout: LayoutState::Maximized,
                ..
            }
        ) {
            match location {
                Location::Top
                | Location::TopLeft
                | Location::TopRight
                | Location::Bottom
                | Location::BottomLeft
                | Location::BottomRight
                | Location::Left
                | Location::Right => Location::None,
                other => other,
            }
        } else {
            location
        };

        Some(match self.mouse_location {
            Location::Top => CursorIcon::NResize,
            Location::TopRight => CursorIcon::NeResize,
            Location::Right => CursorIcon::EResize,
            Location::BottomRight => CursorIcon::SeResize,
            Location::Bottom => CursorIcon::SResize,
            Location::BottomLeft => CursorIcon::SwResize,
            Location::Left => CursorIcon::WResize,
            Location::TopLeft => CursorIcon::NwResize,
            _ => CursorIcon::Default,
        })
    }

    fn click_point_left(&mut self) {
        self.mouse_location = Location::None;
        self.dirty = true;
    }

    fn set_hidden(&mut self, hidden: bool) {
        match (&self.state, hidden) {
            (WeztermFrameState::Hidden, false) => {
                let _ = self.pool.resize(1);
                self.state = WeztermFrameState::Visible {
                    layout: LayoutState::Normal,
                    active: false,
                    render_data: FrameRenderData::new(
                        &self.parent,
                        &self.subcompositor,
                        &self.queue_handle,
                    ),
                };
            }
            (_, true) => {
                self.state = WeztermFrameState::Hidden;
            }
            _ => {}
        }
    }

    fn set_resizable(&mut self, resizable: bool) {
        self.resizable = resizable;
    }

    fn update_state(&mut self, state: WindowState) {
        let active = state.contains(WindowState::ACTIVATED);
        let fullscreen = state.contains(WindowState::FULLSCREEN);
        let maximized = state.contains(WindowState::MAXIMIZED);

        let new_layout = if maximized {
            LayoutState::Maximized
        } else {
            LayoutState::Normal
        };

        let current_state = mem::replace(&mut self.state, WeztermFrameState::Hidden);

        self.state = match current_state {
            WeztermFrameState::Hidden => WeztermFrameState::Hidden,
            WeztermFrameState::Fullscreen => {
                if !fullscreen {
                    let render_data =
                        FrameRenderData::new(&self.parent, &self.subcompositor, &self.queue_handle);
                    self.dirty = true;
                    WeztermFrameState::Visible {
                        layout: new_layout,
                        active,
                        render_data,
                    }
                } else {
                    WeztermFrameState::Fullscreen
                }
            }
            WeztermFrameState::Visible {
                layout,
                active: current_active,
                render_data,
            } => {
                if fullscreen {
                    for part in render_data.parts() {
                        part.surface.attach(None, 0, 0);
                        part.surface.commit();
                    }
                    self.dirty = true;
                    WeztermFrameState::Fullscreen
                } else {
                    let dirty = layout != new_layout || current_active != active;
                    self.dirty |= dirty;
                    WeztermFrameState::Visible {
                        layout: new_layout,
                        active,
                        render_data,
                    }
                }
            }
        };
    }

    fn resize(&mut self, width: NonZeroU32, height: NonZeroU32) {
        let render_data = match &mut self.state {
            WeztermFrameState::Visible { render_data, .. } => render_data,
            _ => panic!("trying to resize non-visible frame"),
        };

        let width = width.get();
        let height = height.get();

        render_data.header.width = width;

        render_data.top_border.width = width + 2 * BORDER_SIZE;

        render_data.bottom_border.width = width + 2 * BORDER_SIZE;
        render_data.bottom_border.pos.y = height as i32;

        render_data.left_border.height = height + HEADER_SIZE;

        render_data.right_border.height = render_data.left_border.height;
        render_data.right_border.pos.x = width as i32;

        self.dirty = true;
        self.should_sync = true;
    }

    fn subtract_borders(
        &self,
        width: NonZeroU32,
        height: NonZeroU32,
    ) -> (Option<NonZeroU32>, Option<NonZeroU32>) {
        if matches!(
            self.state,
            WeztermFrameState::Fullscreen | WeztermFrameState::Hidden
        ) {
            (Some(width), Some(height))
        } else {
            (
                NonZeroU32::new(width.get().saturating_sub(2 * BORDER_SIZE)),
                NonZeroU32::new(height.get().saturating_sub(HEADER_SIZE + 2 * BORDER_SIZE)),
            )
        }
    }

    fn add_borders(&self, width: u32, height: u32) -> (u32, u32) {
        if matches!(
            self.state,
            WeztermFrameState::Fullscreen | WeztermFrameState::Hidden
        ) {
            (width, height)
        } else {
            (
                width + 2 * BORDER_SIZE,
                height + (HEADER_SIZE + 2 * BORDER_SIZE),
            )
        }
    }

    fn is_hidden(&self) -> bool {
        matches!(self.state, WeztermFrameState::Hidden)
    }

    fn location(&self) -> (i32, i32) {
        match &self.state {
            WeztermFrameState::Visible { render_data, .. } => render_data.location(),
            WeztermFrameState::Hidden | WeztermFrameState::Fullscreen => (0, 0),
        }
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn draw(&mut self) -> bool {
        let (active, render_data) = match &mut self.state {
            WeztermFrameState::Visible {
                active,
                render_data,
                ..
            } => (*active, render_data),
            _ => return false,
        };

        // Reset the dirty bit and sync option.
        self.dirty = false;
        let should_sync = mem::take(&mut self.should_sync);

        let fill_color = if active {
            PRIMARY_COLOR_ACTIVE
        } else {
            PRIMARY_COLOR_INACTIVE
        }
        .to_le_bytes();

        for part in render_data.parts() {
            // We don't support fractinal scaling here, so round up.
            let scale = self.scale_factor.ceil() as i32;

            if part.width == 0 || part.height == 0 {
                part.surface.attach(None, 0, 0);
                part.surface.commit();
                continue;
            }

            let (buffer, canvas) = match self.pool.create_buffer(
                part.width as i32 * scale,
                part.height as i32 * scale,
                part.width as i32 * 4 * scale,
                wl_shm::Format::Argb8888,
            ) {
                Ok((buffer, canvas)) => (buffer, canvas),
                Err(_) => continue,
            };

            // Fill the canvas.
            for pixel in canvas.chunks_exact_mut(4) {
                pixel[0] = fill_color[0];
                pixel[1] = fill_color[1];
                pixel[2] = fill_color[2];
                pixel[3] = fill_color[3];
            }

            part.surface.set_buffer_scale(scale);
            if should_sync {
                part.subsurface.set_sync();
            } else {
                part.subsurface.set_desync();
            }

            // Update the subsurface position.
            part.subsurface.set_position(part.pos.x, part.pos.y);

            buffer
                .attach_to(&part.surface)
                .expect("failed to attach the buffer");
            if part.surface.version() >= 4 {
                part.surface.damage_buffer(0, 0, i32::MAX, i32::MAX);
            } else {
                part.surface.damage(0, 0, i32::MAX, i32::MAX);
            }

            part.surface.commit();
        }

        should_sync
    }

    fn update_wm_capabilities(&mut self, capabilities: WindowManagerCapabilities) {
        self.dirty |= self.wm_capabilities != capabilities;
        self.wm_capabilities = capabilities;
    }

    fn set_title(&mut self, _: impl Into<String>) {}
}

/// Inner state to simplify dropping.
#[derive(Debug)]
struct FrameRenderData {
    header: FramePart,
    top_border: FramePart,
    right_border: FramePart,
    bottom_border: FramePart,
    left_border: FramePart,
}

impl FrameRenderData {
    fn new<State>(
        parent: &WlSurface,
        subcompositor: &SubcompositorState,
        queue_handle: &QueueHandle<State>,
    ) -> Self
    where
        State: Dispatch<WlSurface, SurfaceData> + Dispatch<WlSubsurface, SubsurfaceData> + 'static,
    {
        Self {
            header: FramePart::new(
                subcompositor.create_subsurface(parent.clone(), queue_handle),
                0,
                HEADER_SIZE,
                Position {
                    x: 0,
                    y: -(HEADER_SIZE as i32),
                },
                Location::Head,
            ),
            top_border: FramePart::new(
                subcompositor.create_subsurface(parent.clone(), queue_handle),
                0,
                BORDER_SIZE,
                Position {
                    x: -(BORDER_SIZE as i32),
                    y: -(HEADER_SIZE as i32 + BORDER_SIZE as i32),
                },
                Location::Top,
            ),
            right_border: FramePart::new(
                subcompositor.create_subsurface(parent.clone(), queue_handle),
                BORDER_SIZE,
                0,
                Position {
                    x: 0,
                    y: -(HEADER_SIZE as i32),
                },
                Location::Right,
            ),
            bottom_border: FramePart::new(
                subcompositor.create_subsurface(parent.clone(), queue_handle),
                0,
                BORDER_SIZE,
                Position {
                    x: -(BORDER_SIZE as i32),
                    y: 0,
                },
                Location::Bottom,
            ),
            left_border: FramePart::new(
                subcompositor.create_subsurface(parent.clone(), queue_handle),
                BORDER_SIZE,
                0,
                Position {
                    x: -(BORDER_SIZE as i32),
                    y: -(HEADER_SIZE as i32),
                },
                Location::Left,
            ),
        }
    }

    fn parts(&self) -> [&FramePart; 5] {
        [
            &self.header,
            &self.top_border,
            &self.right_border,
            &self.bottom_border,
            &self.left_border,
        ]
    }

    fn location(&self) -> (i32, i32) {
        (self.top_border.pos.x, self.top_border.pos.y)
    }

    fn find_part(&self, surface_id: &ObjectId) -> Option<&FramePart> {
        IntoIterator::into_iter(self.parts()).find(|part| &part.surface.id() == surface_id)
    }
}

#[derive(Debug, Copy, Clone)]
struct Position {
    x: i32,
    y: i32,
}

#[derive(Debug)]
struct FramePart {
    /// The surface used for the frame part.
    subsurface: WlSubsurface,

    /// The surface used for this part.
    surface: WlSurface,

    /// The width of the Frame part in logical pixels.
    width: u32,

    /// The height of the Frame part in logical pixels.
    height: u32,

    /// The position for the subsurface.
    pos: Position,

    /// The base location for this part.
    base_location: Location,
}

impl FramePart {
    fn new(
        surfaces: (WlSubsurface, WlSurface),
        width: u32,
        height: u32,
        pos: Position,
        base_location: Location,
    ) -> Self {
        let (subsurface, surface) = surfaces;
        // XXX sync subsurfaces with the main surface.
        subsurface.set_sync();
        Self {
            surface,
            subsurface,
            width,
            height,
            pos,
            base_location,
        }
    }

    fn precise_location(&self, x: f64) -> Location {
        match self.base_location {
            Location::Top => {
                if x <= f64::from(CORNER_SIZE) {
                    Location::TopLeft
                } else if x >= f64::from(self.width - CORNER_SIZE) {
                    Location::TopRight
                } else {
                    Location::Top
                }
            }
            Location::Bottom => {
                if x <= f64::from(CORNER_SIZE) {
                    Location::BottomLeft
                } else if x >= f64::from(self.width - CORNER_SIZE) {
                    Location::BottomRight
                } else {
                    Location::Bottom
                }
            }
            other => other,
        }
    }
}

impl Drop for FramePart {
    fn drop(&mut self) {
        self.subsurface.destroy();
        self.surface.destroy();
    }
}

/// The location inside the frame.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Location {
    /// The location doesn't belong to the frame.
    None,
    /// Header bar.
    Head,
    /// Top border.
    Top,
    /// Top right corner.
    TopRight,
    /// Right border.
    Right,
    /// Bottom right corner.
    BottomRight,
    /// Bottom border.
    Bottom,
    /// Bottom left corner.
    BottomLeft,
    /// Left border.
    Left,
    /// Top left corner.
    TopLeft,
}
