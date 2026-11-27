class CanonicalFailure {
    static int validate(int code) {
        if (code == 2) throw new RuntimeException("zeta");
        if (code == 1) throw new java.lang.RuntimeException("alpha");
        if (code == 3) throw new java.lang.RuntimeException();
        return code;
    }
}
