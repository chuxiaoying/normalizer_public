record Offer(String code, int price) {}
record Pick(String firstCode, int firstPrice, String secondCode) {}
class Offers {
    static Pick sample(Offer[] offers, Offer backup) {
        Offer same = backup; Offer first;
        /* @normalizer-ir first = choose from offers else same */
        Offer second;
        /* @normalizer-ir second = choose from offers else backup */
        return new Pick(first.code(), first.price(), second.code());
    }
}
