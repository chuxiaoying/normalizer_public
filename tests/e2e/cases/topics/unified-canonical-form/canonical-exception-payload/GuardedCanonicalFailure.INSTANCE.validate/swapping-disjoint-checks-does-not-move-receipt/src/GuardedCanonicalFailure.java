import normalizer.intrinsics.N;

class GuardedCanonicalFailure {
    static final GuardedCanonicalFailure INSTANCE = new GuardedCanonicalFailure(new Classifier());

    @N.External
    private final Classifier classifier;

    GuardedCanonicalFailure(Classifier classifier) {
        this.classifier = classifier;
    }

    int validate(boolean enabled, int code) {
        if (enabled) {
            int rank = classifier.rank(code);
            if (rank == 1) throw new RuntimeException("alpha");
            if (rank == 2) throw new RuntimeException("zeta");
        }
        return code;
    }
}

class Classifier {
    native int rank(int code);
}
