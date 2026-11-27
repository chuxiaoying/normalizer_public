record Offer(String code, int price) {}
record Pick(String firstCode, int firstPrice, String secondCode) {}
class Offers {
    static Pick sample(Offer[] offers, Offer backup) {
        Offer quote;
        /* @normalizer-ir quote = choose from offers else backup */
        Offer second;
        /* @normalizer-ir second = choose from offers else backup */
        return new Pick(quote.code(), quote.price(), second.code());
    }
}
