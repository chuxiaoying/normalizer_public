class CanonicalLabel {
    static String label() {
        // invalid \u00G1
        return "unreachable";
    }
}
