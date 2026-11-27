class CanonicalChecks {
    static boolean approved(boolean address, boolean balance, boolean consent) {
        return (balance || consent) || address;
    }
}
