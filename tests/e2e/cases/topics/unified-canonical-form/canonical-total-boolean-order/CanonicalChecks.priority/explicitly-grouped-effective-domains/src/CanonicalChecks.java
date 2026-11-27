class CanonicalChecks {
    static int priority(boolean a, boolean b, boolean c) {
        if (a || (!b && c)) return 10;
        if (!a && b) return 20;
        return 30;
    }
}
