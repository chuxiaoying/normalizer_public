class CanonicalChecks {
    static boolean approved(boolean address, boolean balance, boolean consent) {
        return consent || (balance || address);
    }
}
