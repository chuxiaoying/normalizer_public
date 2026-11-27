class CanonicalChecks {
    static boolean approved(boolean address, boolean balance, boolean consent) {
        return (address ? consent : !consent) ? true : address;
    }
}
