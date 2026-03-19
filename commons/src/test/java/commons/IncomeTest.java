package commons;

import org.junit.jupiter.api.*;

import static org.junit.jupiter.api.Assertions.*;

public class IncomeTest {

    private Income testIncome1;
    private Income testIncome2;
    private Income testIncome3;

    private final IncomeCategory testCategory = new IncomeCategory();
    private final User testUser = new User();

    @BeforeEach
    void setUp() {
        testIncome1 = new Income("rent", 200.0, testCategory, testUser);
        testIncome2 = new Income("caps", 100.0, testCategory, testUser);
        testIncome3 = new Income("caps", 100.0, testCategory, testUser);
    }

    @Test
    void invalidConstructorTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testIncome1 = new Income(null, 200.0, testCategory, testUser),
                "name null"
        );

        assertThrows(IllegalArgumentException.class, () ->
                testIncome1 = new Income("rent", null, testCategory, testUser),
                "amount null"
        );

        assertThrows(IllegalArgumentException.class, () ->
                testIncome1 = new Income("rent", -1.0, testCategory, testUser),
                "amount not positive"
        );

        assertThrows(IllegalArgumentException.class, () ->
                testIncome1 = new Income("rent", 200.0, null, testUser),
                "category null"
        );

        assertThrows(IllegalArgumentException.class, () ->
                testIncome1 = new Income("rent", 200.0, testCategory, null),
                "user null"
        );
    }

    @Test
    void setValidNameTest() {
        String name = "pats";
        testIncome1.setName(name);
        assertEquals(name, testIncome1.getName());
    }

    @Test
    void setInvalidNameTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testIncome1.setName(null)
        );
    }

    @Test
    void setValidUserTest() {
        User user = new User();
        testIncome1.setUser(user);
        assertEquals(user, testIncome1.getUser());
    }

    @Test
    void setInvalidUserTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testIncome1.setUser(null)
        );
    }

    @Test
    void setValidAmountTest() {
        Double amount = 20.0;
        testIncome1.setAmount(amount);
        assertEquals(amount, testIncome1.getAmount());
    }

    @Test
    void setNullAmountTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testIncome1.setAmount(null)
        );
    }

    @Test
    void setNegativeAmountTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testIncome1.setAmount(-2.0)
        );
    }

    @Test
    void setValidCategoryTest() {
        IncomeCategory testIncomeCategory = new IncomeCategory("car");
        testIncome1.setCategory(testIncomeCategory);
        assertEquals(testIncomeCategory, testIncome1.getCategory());
    }

    @Test
    void setInvalidCategoryTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testIncome1.setCategory(null)
        );
    }

    @Test
    void notEqualsTest() {
        assertNotEquals(testIncome1, testIncome2);
    }

    @Test
    void equalsTest() {
        assertEquals(testIncome3, testIncome2);
    }

    @Test
    void hashCodeDiffTest() {
        assertNotEquals(testIncome1.hashCode(), testIncome2.hashCode());
    }

    @Test
    void hashCodeSameTest() {
        assertEquals(testIncome3.hashCode(), testIncome2.hashCode());
    }

}

