class RouteBox { int amount; }
class StoreRoute {
    static int local(int amount, boolean replace) {
        RouteBox box = new RouteBox(); RouteBox alias = box;
        if (replace) {
            if (amount > 0) { alias.amount = amount; return box.amount + 1; }
        } else {
            box.amount = 2;
        }
        return alias.amount;
    }
}
