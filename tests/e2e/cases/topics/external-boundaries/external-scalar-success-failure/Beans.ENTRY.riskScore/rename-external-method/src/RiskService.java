import normalizer.intrinsics.N;

class RiskService {
    @N.External
    private final RiskClient riskClient;

    RiskService(RiskClient riskClient) {
        this.riskClient = riskClient;
    }

    int riskScore(int accountId) {
        try {
            return riskClient.readScore(accountId);
        } catch (RiskUnavailable ignored) {
            return 0;
        }
    }
}

class RiskClient {
    native int readScore(int accountId) throws RiskUnavailable;
}

class RiskUnavailable extends RuntimeException {}

class Beans {
    static final RiskService ENTRY = new RiskService(new RiskClient());
}
