class CaseParity {
    static int evaluate(boolean enabled, int a, int b, int shift, boolean include) {
        int chosen = !enabled ? b : a;
        return (chosen + shift) * (chosen - shift);
    }
}
