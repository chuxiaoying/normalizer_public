import normalizer.intrinsics.N;

class RemoteError extends RuntimeException {}
class DisjointError extends RuntimeException {}
class TerminalError extends RuntimeException {}

class FinallyService {
    static final FinallyService INSTANCE = new FinallyService(new ScoreClient());

    @N.External
    private final ScoreClient client;
    @N.EntryState
    int attempts;

    FinallyService(ScoreClient client) { this.client = client; }

    int read(int accountId) {
        try {
            return client.fetch(accountId);
        } catch (DisjointError ignored) {
            return 0;
        } finally {
            attempts = attempts + 1; throw new TerminalError();
        }
    }
}

class ScoreClient {
    native int fetch(int accountId) throws RemoteError;
}
