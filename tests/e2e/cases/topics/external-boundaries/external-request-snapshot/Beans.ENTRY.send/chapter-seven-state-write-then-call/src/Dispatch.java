import normalizer.intrinsics.N;

class Beans {
    static final Dispatch DISPATCH = new Dispatch(new OrderPort(), 10);
}

class Dispatch {
    @N.EntryState
    private int remaining;
    @N.External
    private final OrderPort orders;

    Dispatch(OrderPort orders, int initialRemaining) {
        this.orders = orders;
        this.remaining = initialRemaining;
    }

    int place(int units) {
        remaining = remaining - units;
        return orders.create(units, remaining);
    }
}

class OrderPort {
    native int create(int units, int remaining) throws OrderUnavailable;
}

class OrderUnavailable extends RuntimeException {}
