class Catalog {
    static int price() { return Price.price(); }
    static int price(int offer) { return offer + 99; }
}
class Price {
    static int price() { return 12; }
}
class OtherPrice {
    static int price() { return 99; }
}
