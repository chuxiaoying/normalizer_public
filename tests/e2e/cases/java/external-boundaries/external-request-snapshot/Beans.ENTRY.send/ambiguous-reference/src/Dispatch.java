import normalizer.intrinsics.N;

class Draft {
    int sku;
    int units;
}

class Dispatch {
    @N.External
    private final Broker broker;

    Dispatch(Broker broker) {
        this.broker = broker;
    }

    void submit(Draft request) {
        broker.send(request);
    }

    int send(int sku, int units, int revisedUnits) {
        Draft draft = new Draft();
        draft.sku = sku;
        draft.units = units;
        Draft alias = draft;
        submit(draft);
        alias.units = revisedUnits;
        submit(draft);
        return draft.units;
    }
}

class Broker {
    native void send(Draft request);
}

class Beans {
    static final Dispatch ENTRY = new Dispatch(new Broker());
}
