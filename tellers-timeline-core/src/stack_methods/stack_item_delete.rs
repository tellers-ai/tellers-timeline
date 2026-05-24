use crate::{Item, Stack};

impl Stack {
    /// Delete an item by id across all tracks. Linked clips with the same Resolve
    /// link group are deleted too. If replace_with_gap is true and a removed item
    /// has a positive duration, a gap of equal duration is inserted.
    /// Returns removed items with their source track indices.
    pub fn delete_item(&mut self, item_id: &str, replace_with_gap: bool) -> Vec<(usize, Item)> {
        let link_group_id = match self.get_item(item_id).and_then(|(_, _, item)| match item {
            Item::Clip(clip) => super::resolve_link_group_id(&clip.metadata),
            Item::Gap(_) => None,
        }) {
            Some(id) => id,
            None => {
                let removed: Vec<_> = self
                    .delete_one_item(item_id, replace_with_gap)
                    .into_iter()
                    .collect();
                if !removed.is_empty() {
                    self.sanitize();
                }
                return removed;
            }
        };

        let removed = self.delete_link_group(link_group_id, replace_with_gap);
        if !removed.is_empty() {
            self.sanitize();
        }
        removed
    }
}
