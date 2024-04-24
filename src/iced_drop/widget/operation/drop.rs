use iced::{
    advanced::widget::{operation::Outcome, Id, Operation},
    Rectangle,
};

/// Produces an [`Operation`] that will find the drop zones that pass a filter on the zone's bounds.
/// For any drop zone to be considered, the Element must have some Id.
/// If `options` is `None`, all drop zones will be considered.
/// Depth determines how how deep into nested drop zones to go.
/// If 'depth' is `None`, nested dropzones will be fully explored
pub fn find_zones<F>(
    filter: F,
    options: Option<Vec<Id>>,
    depth: Option<usize>,
) -> impl Operation<Vec<(Id, Rectangle)>>
where
    F: Fn(&Rectangle) -> bool + 'static,
{
    struct FindDropZone<F> {
        filter: F,
        options: Option<Vec<Id>>,
        zones: Vec<(Id, Rectangle)>,
        max_depth: Option<usize>,
        c_depth: usize,
    }

    impl<F> Operation<Vec<(Id, Rectangle)>> for FindDropZone<F>
    where
        F: Fn(&Rectangle) -> bool + 'static,
    {
        fn container(
            &mut self,
            id: Option<&Id>,
            bounds: iced::Rectangle,
            operate_on_children: &mut dyn FnMut(&mut dyn Operation<Vec<(Id, Rectangle)>>),
        ) {
            match id {
                Some(id) => {
                    let is_option = match &self.options {
                        Some(options) => options.contains(id),
                        None => true,
                    };
                    if is_option && (self.filter)(&bounds) {
                        self.c_depth += 1;
                        self.zones.push((id.clone(), bounds));
                    }
                }
                None => (),
            }
            let goto_next = match &self.max_depth {
                Some(m_depth) => self.c_depth < *m_depth,
                None => true,
            };
            if goto_next {
                operate_on_children(self);
            }
        }

        fn finish(&self) -> Outcome<Vec<(Id, Rectangle)>> {
            Outcome::Some(self.zones.clone())
        }
    }

    FindDropZone {
        filter,
        options,
        zones: vec![],
        max_depth: depth,
        c_depth: 0,
    }
}
