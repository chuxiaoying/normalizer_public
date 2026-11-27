class CanonicalFailure {
    static int validate(int code) {
        if (code == 1) return 10;
        if (code == 2) return 20;
        if (code == 3) return 30;
        throw new RuntimeException("invalid");
    }
}
