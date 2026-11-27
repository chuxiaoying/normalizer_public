class CanonicalFailure {
    static int validate(int code) {
        if (code == 1) throw new RuntimeException("alpha");
        if (code == 2) throw new RuntimeException("zeta");
        if (code == 3) throw new RuntimeException();
        return code;
    }
}
