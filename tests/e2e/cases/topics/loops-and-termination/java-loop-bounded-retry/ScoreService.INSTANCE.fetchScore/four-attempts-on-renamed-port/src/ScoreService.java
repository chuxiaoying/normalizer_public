import normalizer.intrinsics.N;

class ScoreService {
    static final ScoreService INSTANCE = new ScoreService(new ScoreClient());

    @N.External
    private final ScoreClient scoreClient;

    ScoreService(ScoreClient scoreClient) {
        this.scoreClient = scoreClient;
    }

    int fetchScore(int accountId, int availableCredits) {
        for (int attempt = 0; attempt < 4; attempt++) {
            try {
                return scoreClient.readPoints(accountId);
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
    native int readPoints(int accountId) throws RetryableScoreError;
}

class RetryableScoreError extends RuntimeException {}
