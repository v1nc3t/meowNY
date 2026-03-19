package commons;

import org.junit.jupiter.api.*;

import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.*;

public class UserTest {

    private User testUser1;
    private User testUser2;
    private User testUser3;

    @BeforeEach
    void setUp() {
        testUser1 = new User("Will", "Born", "wb@gmail.com", "wlbr", "test");
        testUser2 = new User("Max", "Smith", "ms@gmail.com", "mxSm", "test");
        testUser3 = new User("Max", "Smith", "ms@gmail.com", "mxSm", "test");
    }

    @Test
    void setIdTest() {
        Long id = 1L;
        testUser1.setId(id);
        assertEquals(id, testUser1.getId());
    }

    @Test
    void setValidFirstNameTest() {
        String firstName = "Feb";
        testUser1.setFirstName(firstName);
        assertEquals(firstName, testUser1.getFirstName());
    }

    @Test
    void setInvalidFirstNameTest() {
        assertThrows(IllegalArgumentException.class, () -> testUser1.setFirstName(null));
    }

    @Test
    void setValidLastNameTest() {
        String lastName = "Feb";
        testUser1.setLastName(lastName);
        assertEquals(lastName, testUser1.getLastName());
    }

    @Test
    void setInvalidLastNameTest() {
        assertThrows(IllegalArgumentException.class, () -> testUser1.setLastName(null));
    }

    @Test
    void getFullNameTest() {
        assertEquals("Max Smith", testUser2.getFullName());
    }

    @Test
    void setEmailTest() {
        String email = "test@gmail.com";
        testUser1.setEmail(email);
        assertEquals(email, testUser1.getEmail());
    }

    @Test
    void setNullEmailTest() {
        testUser1.setEmail(null);
        assertNull(testUser1.getEmail());
    }

    @Test
    void setValidUsernameTest() {
        String username = "testusr";
        testUser1.setUsername(username);
        assertEquals(username, testUser1.getUsername());
    }

    @Test
    void setInvalidUsernameTest() {
        assertThrows(IllegalArgumentException.class, () -> testUser1.setUsername(null));
    }

    @Test
    void setValidPasswordTest() {
        String password = "incorrect";
        testUser1.setPassword(password);
        assertEquals(password, testUser1.getPassword());
    }

    @Test
    void setInvalidPasswordTest() {
        assertThrows(IllegalArgumentException.class, () -> testUser1.setPassword(null));
    }

    @Test
    void incomesListTest() {
        List<Income> incomes = new ArrayList<>();
        testUser1.setIncomes(incomes);
        assertEquals(incomes, testUser1.getIncomes());
    }

    @Test
    void expenseListTest() {
        List<Expense> expenses = new ArrayList<>();
        testUser1.setExpenses(expenses);
        assertEquals(expenses, testUser1.getExpenses());
    }

    @Test
    void budgetsListTest() {
        List<Budget> budgets = new ArrayList<>();
        testUser1.setBudgets(budgets);
        assertEquals(budgets, testUser1.getBudgets());
    }

    @Test
    void notEqualsTest() {
        assertNotEquals(testUser1, testUser2);
    }

    @Test
    void equalsTest() {
        assertEquals(testUser3, testUser2);
    }

    @Test
    void hashCodeDiffTest() {
        assertNotEquals(testUser1.hashCode(), testUser2.hashCode());
    }

    @Test
    void hashCodeSameTest() {
        assertEquals(testUser3.hashCode(), testUser2.hashCode());
    }

}
