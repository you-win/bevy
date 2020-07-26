use crate::Node;
use bevy_ecs::{Changed, Entity, Query, Res, ResMut, With, Without};
use bevy_math::Vec2;
use bevy_transform::prelude::{Children, LocalTransform, Parent};
use bevy_window::{Window, WindowId, Windows};
use std::collections::HashMap;
use stretch::{
    geometry::Size,
    result::Layout,
    style::{Dimension, Style},
    Stretch,
};

pub struct FlexSurface {
    entity_to_stretch: HashMap<Entity, stretch::node::Node>,
    stretch_to_entity: HashMap<stretch::node::Node, Entity>,
    window_nodes: HashMap<WindowId, stretch::node::Node>,
    stretch: Stretch,
}

impl Default for FlexSurface {
    fn default() -> Self {
        Self {
            entity_to_stretch: Default::default(),
            stretch_to_entity: Default::default(),
            window_nodes: Default::default(),
            stretch: Stretch::new(),
        }
    }
}

impl FlexSurface {
    pub fn upsert_node(&mut self, entity: Entity, style: &Style) {
        let mut added = false;
        let stretch = &mut self.stretch;
        let stretch_to_entity = &mut self.stretch_to_entity;
        let stretch_node = self.entity_to_stretch.entry(entity).or_insert_with(|| {
            added = true;
            let stretch_node = stretch.new_node(style.clone(), Vec::new()).unwrap();
            stretch_to_entity.insert(stretch_node, entity);
            stretch_node
        });

        if !added {
            self.stretch
                .set_style(*stretch_node, style.clone())
                .unwrap();
        }
    }

    pub fn update_children(&mut self, entity: Entity, children: &Children) {
        let mut stretch_children = Vec::with_capacity(children.len());
        for child in children.iter() {
            let stretch_node = self.entity_to_stretch.get(child).unwrap();
            stretch_children.push(*stretch_node);
        }

        let stretch_node = self.entity_to_stretch.get(&entity).unwrap();
        self.stretch
            .set_children(*stretch_node, stretch_children)
            .unwrap();
    }

    pub fn update_window(&mut self, window: &Window) {
        let stretch = &mut self.stretch;
        let node = self.window_nodes.entry(window.id).or_insert_with(|| {
            stretch
                .new_node(
                    Style {
                        ..Default::default()
                    },
                    Vec::new(),
                )
                .unwrap()
        });

        stretch
            .set_style(
                *node,
                Style {
                    size: Size {
                        width: Dimension::Points(window.width as f32),
                        height: Dimension::Points(window.height as f32),
                    },
                    ..Default::default()
                },
            )
            .unwrap();
    }

    pub fn set_window_children(
        &mut self,
        window_id: WindowId,
        children: impl Iterator<Item = Entity>,
    ) {
        let stretch_node = self.window_nodes.get(&window_id).unwrap();
        let child_nodes = children
            .map(|e| *self.entity_to_stretch.get(&e).unwrap())
            .collect::<Vec<stretch::node::Node>>();
        self.stretch
            .set_children(*stretch_node, child_nodes)
            .unwrap();
    }

    pub fn compute_window_layouts(&mut self) {
        for window_node in self.window_nodes.values() {
            self.stretch
                .compute_layout(*window_node, stretch::geometry::Size::undefined())
                .unwrap();
        }
    }

    pub fn get_layout(&self, entity: Entity) -> Result<&Layout, stretch::Error> {
        let stretch_node = self.entity_to_stretch.get(&entity).unwrap();
        self.stretch.layout(*stretch_node)
    }
}

// SAFE: as long as MeasureFunc is Send + Sync. https://github.com/vislyhq/stretch/issues/69
unsafe impl Send for FlexSurface {}
unsafe impl Sync for FlexSurface {}

pub fn flex_node_system(
    windows: Res<Windows>,
    mut flex_surface: ResMut<FlexSurface>,
    mut root_node_query: Query<With<Node, Without<Parent, Entity>>>,
    mut node_query: Query<With<Node, (Entity, Changed<Style>)>>,
    mut children_query: Query<With<Node, (Entity, Changed<Children>)>>,
    mut node_transform_query: Query<(Entity, &mut Node, &mut LocalTransform, Option<&Parent>)>,
) {
    // update window root nodes
    for window in windows.iter() {
        flex_surface.update_window(window);
    }

    // update changed nodes
    for (entity, style) in &mut node_query.iter() {
        // TODO: remove node from old hierarchy if its root has changed
        flex_surface.upsert_node(entity, &style);
    }

    // TODO: handle removed nodes

    // update window children (for now assuming all Nodes live in the primary window)
    if let Some(primary_window) = windows.get_primary() {
        flex_surface.set_window_children(primary_window.id, root_node_query.iter().iter());
    }

    // update children
    for (entity, children) in &mut children_query.iter() {
        flex_surface.update_children(entity, &children);
    }

    // compute layouts
    flex_surface.compute_window_layouts();

    for (entity, mut node, mut local, parent) in &mut node_transform_query.iter() {
        let layout = flex_surface.get_layout(entity).unwrap();
        node.size = Vec2::new(layout.size.width, layout.size.height);
        let mut position = local.w_axis();
        position.set_x(layout.location.x + layout.size.width / 2.0);
        position.set_y(layout.location.y + layout.size.height / 2.0);
        if let Some(parent) = parent {
            if let Ok(parent_layout) = flex_surface.get_layout(parent.0) {
                *position.x_mut() -= parent_layout.size.width / 2.0;
                *position.y_mut() -= parent_layout.size.height / 2.0;
            }
        }

        local.set_w_axis(position);
    }
}