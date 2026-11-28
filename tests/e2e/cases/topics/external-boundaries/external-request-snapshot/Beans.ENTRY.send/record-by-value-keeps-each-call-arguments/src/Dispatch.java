import normalizer.intrinsics.N;

record Draft(int sku, int units) {}

class Dispatch {
    @N.External
    private final Broker broker;

    Dispatch(Broker broker) {
        this.broker = broker;
    }

    int send(int sku, int units, int revisedUnits) {
        Draft draft = new Draft(sku, units);
        broker.send(draft);
        draft = new Draft(sku, revisedUnits);
        broker.send(draft);
        return draft.units();
    }
}

class Broker {
    native void send(Draft draft);
}

class Beans {
    static final Dispatch ENTRY = new Dispatch(new Broker());
}
