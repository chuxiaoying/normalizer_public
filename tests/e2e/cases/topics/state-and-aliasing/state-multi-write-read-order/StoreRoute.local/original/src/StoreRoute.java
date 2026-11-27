class RouteBox { int amount; }
class StoreRoute {
    static int local(int amount, boolean replace) {
        RouteBox box = new RouteBox();
        if (replace) {
            if (amount > 0) box.amount = amount;
        } else {
            box.amount = 2;
        }
        return box.amount;
    }
}
