//! Display-mode interlocks shared by the menu, the toolbars and the settings dialog.
use crate::state::Store;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Flat,
    Land,
    Cart,
    Cyl,
    Graph,
    Scatter,
    Panel,
    Vector,
    Toko,
    Point,
    Axis,
    Grid,
    Legend,
    Label,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ViewState {
    pub land: bool,
    pub cart: bool,
    pub cyl: bool,
    pub graph: bool,
    pub scatter: bool,
    pub panel: bool,
    pub vector: bool,
    pub toko: bool,
    pub point: bool,
    pub axis: bool,
    pub grid: bool,
    pub legend: bool,
    pub label: bool,
}

impl Default for ViewState {
    /// Initial: flat, cart, graph, legend, label.
    fn default() -> Self {
        ViewState {
            land: false,
            cart: true,
            cyl: false,
            graph: true,
            scatter: false,
            panel: false,
            vector: false,
            toko: false,
            point: false,
            axis: false,
            grid: false,
            legend: true,
            label: true,
        }
    }
}

/// Items that are switched off when `item` is switched on.
pub fn clears_when_on(item: Item) -> &'static [Item] {
    use Item::*;
    match item {
        Land => &[Vector, Toko, Panel],
        Graph => &[Scatter, Panel],
        Scatter => &[Graph, Panel, Toko],
        Panel => &[Graph, Scatter, Vector, Toko, Point, Land],
        Vector => &[Panel, Land],
        Toko => &[Scatter, Panel, Land],
        Point => &[Panel],
        _ => &[],
    }
}

/// (item, control name of the matching settings-dialog checkbox/radio)
pub const DIALOG_NAMES: [(Item, &str); 12] = [
    (Item::Land, "CBland"),
    (Item::Cart, "RBcartesian"),
    (Item::Cyl, "RBcylindrical"),
    (Item::Graph, "CBgraph"),
    (Item::Scatter, "CBscatter"),
    (Item::Panel, "CBpanel"),
    (Item::Vector, "CBvector"),
    (Item::Toko, "CBtoko"),
    (Item::Point, "CBpoint"),
    (Item::Axis, "CBaxis"),
    (Item::Grid, "CBgrid"),
    (Item::Legend, "CBlegend"),
];

impl ViewState {
    pub fn get(&self, item: Item) -> bool {
        match item {
            Item::Flat => !self.land,
            Item::Land => self.land,
            Item::Cart => self.cart,
            Item::Cyl => self.cyl,
            Item::Graph => self.graph,
            Item::Scatter => self.scatter,
            Item::Panel => self.panel,
            Item::Vector => self.vector,
            Item::Toko => self.toko,
            Item::Point => self.point,
            Item::Axis => self.axis,
            Item::Grid => self.grid,
            Item::Legend => self.legend,
            Item::Label => self.label,
        }
    }

    fn raw_set(&mut self, item: Item, on: bool) {
        match item {
            Item::Flat => self.land = !on,
            Item::Land => self.land = on,
            Item::Cart => self.cart = on,
            Item::Cyl => self.cyl = on,
            Item::Graph => self.graph = on,
            Item::Scatter => self.scatter = on,
            Item::Panel => self.panel = on,
            Item::Vector => self.vector = on,
            Item::Toko => self.toko = on,
            Item::Point => self.point = on,
            Item::Axis => self.axis = on,
            Item::Grid => self.grid = on,
            Item::Legend => self.legend = on,
            Item::Label => self.label = on,
        }
    }

    /// Set a value as a dialog check box does; turning ON clears the related items.
    pub fn set(&mut self, item: Item, on: bool) {
        if on {
            for &c in clears_when_on(item) {
                self.raw_set(c, false);
            }
        }
        match item {
            Item::Cart if on => self.cyl = false,
            Item::Cyl if on => self.cart = false,
            _ => {}
        }
        self.raw_set(item, on);
    }

    /// Menu / toolbar click.
    pub fn click(&mut self, item: Item) {
        match item {
            // flat and land both toggle "land"
            Item::Flat | Item::Land => self.set(Item::Land, !self.land),
            Item::Cart | Item::Cyl => self.set(item, true),
            _ => self.set(item, !self.get(item)),
        }
    }

    pub fn from_store(s: &Store) -> ViewState {
        let c = |n: &str| s.checked(n);
        ViewState {
            land: c("CBland"),
            cart: c("RBcartesian"),
            cyl: c("RBcylindrical"),
            graph: c("CBgraph"),
            scatter: c("CBscatter"),
            panel: c("CBpanel"),
            vector: c("CBvector"),
            toko: c("CBtoko"),
            point: c("CBpoint"),
            axis: c("CBaxis"),
            grid: c("CBgrid"),
            legend: c("CBlegend"),
            label: c("CBLabel"),
        }
    }

    pub fn write_store(&self, s: &mut Store) {
        for (item, name) in DIALOG_NAMES {
            s.set_checked(name, self.get(item));
        }
        s.set_checked("CBLabel", self.label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Item::*;

    fn all_on() -> ViewState {
        ViewState { land: true, cart: true, cyl: false, graph: true, scatter: true, panel: true, vector: true, toko: true, point: true, axis: true, grid: true, legend: true, label: true }
    }

    #[test]
    fn initial_state() {
        let v = ViewState::default();
        assert!(v.get(Flat) && !v.get(Land) && v.get(Cart) && v.get(Graph) && v.get(Legend) && v.get(Label));
        assert!(!v.scatter && !v.panel && !v.vector && !v.toko && !v.point && !v.axis && !v.grid);
    }

    #[test]
    fn flat_and_land_toggle_land() {
        let mut v = ViewState::default();
        v.click(Flat);
        assert!(v.land);
        v.click(Flat);
        assert!(!v.land);
        v.click(Land);
        assert!(v.land);
        v.click(Land);
        assert!(!v.land);
    }

    #[test]
    fn land_on_clears_and_off_does_not() {
        let mut v = all_on();
        v.land = false;
        v.click(Land);
        assert!(v.land && !v.vector && !v.toko && !v.panel);
        assert!(v.graph && v.scatter && v.point);
        let mut w = all_on();
        w.click(Land); // turn off
        assert!(!w.land && w.vector && w.toko && w.panel && w.graph);
    }

    #[test]
    fn cart_cyl_radio() {
        let mut v = ViewState::default();
        v.click(Cyl);
        assert!(v.cyl && !v.cart);
        v.click(Cyl);
        assert!(v.cyl && !v.cart);
        v.click(Cart);
        assert!(v.cart && !v.cyl);
    }

    fn check_on(item: Item, cleared: &[Item], kept: &[Item]) {
        let mut v = all_on();
        v.land = true;
        v.raw_set(item, false);
        v.click(item);
        assert!(v.get(item), "{item:?} should be on");
        for c in cleared {
            assert!(!v.get(*c), "{item:?} on must clear {c:?}");
        }
        for k in kept {
            assert!(v.get(*k), "{item:?} on must keep {k:?}");
        }
        // clicking again turns it off and restores nothing
        let before = v.clone();
        v.click(item);
        assert!(!v.get(item));
        for it in [Land, Graph, Scatter, Panel, Vector, Toko, Point, Axis, Grid, Legend, Label] {
            if it != item {
                assert_eq!(v.get(it), before.get(it));
            }
        }
    }

    #[test]
    fn graph_rule() {
        check_on(Graph, &[Scatter, Panel], &[Vector, Toko, Point, Land]);
    }
    #[test]
    fn scatter_rule() {
        check_on(Scatter, &[Graph, Panel, Toko], &[Vector, Point, Land]);
    }
    #[test]
    fn panel_rule() {
        check_on(Panel, &[Graph, Scatter, Vector, Toko, Point, Land], &[Axis, Grid, Legend, Label]);
    }
    #[test]
    fn vector_rule() {
        check_on(Vector, &[Panel, Land], &[Graph, Scatter, Toko, Point]);
    }
    #[test]
    fn toko_rule() {
        check_on(Toko, &[Scatter, Panel, Land], &[Graph, Vector, Point]);
    }
    #[test]
    fn point_rule() {
        check_on(Point, &[Panel], &[Graph, Scatter, Vector, Toko, Land]);
    }

    #[test]
    fn independent_items() {
        for it in [Axis, Grid, Legend, Label] {
            let mut v = all_on();
            v.raw_set(it, false);
            let before = v.clone();
            v.click(it);
            assert!(v.get(it));
            for o in [Land, Graph, Scatter, Panel, Vector, Toko, Point] {
                assert_eq!(v.get(o), before.get(o));
            }
            v.click(it);
            assert!(!v.get(it));
        }
    }

    #[test]
    fn exclusive_trio_may_be_all_off() {
        let mut v = ViewState::default();
        v.click(Graph);
        assert!(!v.graph && !v.scatter && !v.panel);
    }

    #[test]
    fn dialog_set_uses_same_rules() {
        let mut v = ViewState::default();
        v.set(Panel, true);
        assert!(v.panel && !v.graph);
        v.set(Land, true);
        assert!(v.land && !v.panel);
        v.set(Land, false);
        assert!(!v.land);
    }
}
