import normalizer.intrinsics.N;

class Pass {
    @N.External
    private final RiskClient client;

    Pass(RiskClient client) {
        this.client = client;
    }

    int read(int accountId) {
        return client.fetch(accountId);
    }
}

class RiskClient {
    native int fetch(int accountId) throws RiskUnavailable;
}

class RiskUnavailable extends RuntimeException {}

class Beans {
    static final Pass ENTRY = new Pass(new RiskClient());
}
