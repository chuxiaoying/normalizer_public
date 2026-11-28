class CanonicalArray {
    static int digits(boolean first) {
        int[] left = {7};
        int[] right = {};
        int[] selected = first ? left : right;
        return selected[0];
    }
}
