import normalizer.intrinsics.N;

class ReadinessService {
    static final ReadinessService INSTANCE = new ReadinessService(new ReadinessClient());
    @N.External
    private final ReadinessClient readinessClient;

    ReadinessService(ReadinessClient readinessClient) {
        this.readinessClient = readinessClient;
    }

    int readWhenReady(int accountId) {
        for (; !readinessClient.isReady(accountId); ) {
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
    native boolean isReady(int accountId);
    native void waitForChange(int accountId);
    native int readScore(int accountId);
}
