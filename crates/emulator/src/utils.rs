use tycho_types::cell::{Cell, CellBuilder, CellFamily, Store};

pub trait StoreExt: Store {
    fn to_cell(&self) -> Cell;
}

impl<T: Store + ?Sized> StoreExt for T {
    fn to_cell(&self) -> Cell {
        let mut builder = CellBuilder::new();
        self.store_into(&mut builder, Cell::empty_context())
            .expect("Failed to store data into cell builder");
        builder.build().expect("Failed to build cell from builder")
    }
}
