class CaseParity {
    static int evaluate(boolean enabled, int a, int b, int shift, boolean include) {
        int selected = enabled ? a : b;
        return (selected + shift) * (selected - shift);
    }
}
