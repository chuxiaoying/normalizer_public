class CanonicalArray {
    static boolean digits(boolean first) {
        int[] left = {7};
        int[] right = {7};
        int[] selected = first ? left : right;
        return selected == left;
    }
}
