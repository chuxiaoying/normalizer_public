import normalizer.intrinsics.N;

class ScoreService {
    static final ScoreService INSTANCE = new ScoreService(new ScoreClient());

    @N.External
    private final ScoreClient scoreClient;

    ScoreService(ScoreClient scoreClient) {
        this.scoreClient = scoreClient;
    }

    int fetchScore(int accountId, int availableCredits) {
        for (int attempt = 0; attempt < 1; attempt++) {
            try {
                return scoreClient.fetchScore(accountId);
            } catch (RetryableScoreError ignored) {
                // retry
            }
        }
        int fallbackScore = 0;
        /* @normalizer-ir
           fallbackScore += count from [0, availableCredits) as credit
        */
        return fallbackScore;
    }
}

class ScoreClient {
    native int fetchScore(int accountId) throws RetryableScoreError;
}

class RetryableScoreError extends RuntimeException {}
