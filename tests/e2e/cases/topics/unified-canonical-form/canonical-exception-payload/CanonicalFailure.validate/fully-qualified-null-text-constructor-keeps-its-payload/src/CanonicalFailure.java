class CanonicalFailure {
    static int validate(int code) {
        if (code == 2) throw new RuntimeException("zeta");
        if (code == 1) throw new java.lang.RuntimeException((java.lang.String) null);
        if (code == 3) throw new RuntimeException();
        return code;
    }
}
