class RouteBox { int amount; }
class StoreRoute {
    static int local(int amount, boolean replace) {
        RouteBox box = new RouteBox();
        if (replace) {
            if (0 < amount) box.amount = amount;
        } else {
            box.amount = 2;
        }
        return box.amount;
    }
}
