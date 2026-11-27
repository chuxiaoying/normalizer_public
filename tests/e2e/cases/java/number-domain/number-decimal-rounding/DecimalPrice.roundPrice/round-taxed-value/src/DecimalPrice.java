import java.math.BigDecimal;
import normalizer.intrinsics.N;

class DecimalPrice {
    static BigDecimal roundPrice(BigDecimal value) {
        BigDecimal factor; /* @normalizer-ir factor = 1.13 */ return N.round(N.multiply(value, factor), 2, N.HALF_UP);
    }
}
