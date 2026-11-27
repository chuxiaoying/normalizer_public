import normalizer.intrinsics.N;

class RiskService {
    @N.External
    private final RiskClient riskClient;

    RiskService(RiskClient riskClient) {
        this.riskClient = riskClient;
    }

    int riskScore(int accountId) {
        try {
            return riskClient.fetchScore(accountId);
        } catch (Timeout ignored) {
            return 0;
        }
    }
}

class RiskClient {
    native int fetchScore(int accountId) throws Timeout;
}

class Timeout extends RuntimeException {}

class Beans {
    static final RiskService ENTRY = new RiskService(new RiskClient());
}
