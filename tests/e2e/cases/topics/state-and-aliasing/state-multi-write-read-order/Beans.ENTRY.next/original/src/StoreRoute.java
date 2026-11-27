import normalizer.intrinsics.N;

class RouteQuota { int units; }
class StoreRoute {
    @N.EntryState
    RouteQuota quota;
    int next(int units, boolean replace) {
        if (replace) {
            if (units > 0) quota.units = units;
        } else {
            quota.units = 2;
        }
        return quota.units;
    }
}

class Beans {
    static final StoreRoute ENTRY = new StoreRoute();
    static {
        ENTRY.quota = new RouteQuota();
    }
}
