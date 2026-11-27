import normalizer.intrinsics.N;

class ReadinessService {
    static final ReadinessService INSTANCE = new ReadinessService();
    int readWhenReady(int accountId) {
        ReadinessClient readinessClient = new ReadinessClient();
        while (!readinessClient.isReady(accountId)) {
            readinessClient.waitForChange(accountId);
        }
        return readinessClient.readScore(accountId);
    }

    int recursiveNormalize(int value) {
        if (value <= 0) {
            return 0;
        }
        return recursiveNormalize(value - 1) + 1;
    }
}

class ReadinessClient {
    boolean isReady(int accountId) { return accountId > 0; }
    void waitForChange(int accountId) {}
    int readScore(int accountId) { return accountId; }
}
