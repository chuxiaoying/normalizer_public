class CanonicalChecks {
    static int priority(boolean a, boolean b, boolean c) {
        if (a) return 10;
        if (a || b) return 20;
        if (a || c) return 10;
        return 30;
    }
}
