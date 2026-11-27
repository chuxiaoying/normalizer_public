import normalizer.intrinsics.N;

class RemoteError extends RuntimeException {}

class LocalResource implements java.lang.AutoCloseable {
    private final FinallyService owner;
    private final int increment;
    LocalResource(FinallyService owner, int increment) {
        this.owner = owner;
        this.increment = increment;
    }
    public void close() { owner.attempts = owner.attempts + increment; }
}

class FinallyService {
    static final FinallyService INSTANCE = new FinallyService(new ScoreClient());

    @N.External
    private final ScoreClient client;
    @N.EntryState
    int attempts;

    FinallyService(ScoreClient client) { this.client = client; }

    int read(int accountId) {
        try (LocalResource resource = new LocalResource(this, 10)) {
            return client.fetch(accountId);
        } catch (RemoteError ignored) {
            return 0;
        } finally {
            attempts = attempts + 1;
        }
    }
}

class ScoreClient {
    native int fetch(int accountId) throws RemoteError;
}
