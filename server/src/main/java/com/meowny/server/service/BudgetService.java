package com.meowny.server.service;

import com.meowny.server.dto.budget.BudgetResponse;
import com.meowny.server.dto.budget.CreateBudgetRequest;
import com.meowny.server.dto.budget.UpdateBudgetRequest;
import com.meowny.server.entity.Budget;
import com.meowny.server.entity.BudgetScope;
import com.meowny.server.entity.Category;
import com.meowny.server.entity.TransactionType;
import com.meowny.server.entity.User;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.BudgetRepository;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.repository.UserRepository;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.time.LocalDate;
import java.util.List;
import java.util.stream.Collectors;

@Service
public class BudgetService {

    private final BudgetRepository budgetRepository;
    private final UserRepository userRepository;
    private final CategoryRepository categoryRepository;

    public BudgetService(BudgetRepository budgetRepository,
                         UserRepository userRepository,
                         CategoryRepository categoryRepository
    ) {
        this.budgetRepository = budgetRepository;
        this.userRepository = userRepository;
        this.categoryRepository = categoryRepository;
    }

    @Transactional(readOnly = true)
    public BudgetResponse getBudgetById(Long id) {
        Budget budget = budgetRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Budget not found with ID:" + id));
        return mapToResponse(budget);
    }

    @Transactional(readOnly = true)
    public List<BudgetResponse> getBudgetsByPeriod(Long userId, Integer year, Integer month) {
        LocalDate effectiveFrom = LocalDate.of(year, month, 1);
        return budgetRepository.findByUserIdAndEffectiveFrom(userId, effectiveFrom)
                .stream()
                .map(this::mapToResponse)
                .collect(Collectors.toList());
    }

    @Transactional
    public BudgetResponse createBudget(CreateBudgetRequest request) {
        User user = userRepository.findById(request.userId())
                .orElseThrow(() -> new IllegalArgumentException("User not found with ID: " + request.userId()));

        LocalDate effectiveFrom = request.effectiveFrom().withDayOfMonth(1);
        Category category = resolveCategory(request);

        if (request.scope() == BudgetScope.GLOBAL) {
            budgetRepository.findGlobalByUserIdAndEffectiveFrom(request.userId(), effectiveFrom)
                    .ifPresent(existing -> {
                        throw new ResourceConflictException(
                                "A global budget limit is already defined for " + effectiveFrom + "."
                        );
                    });
        } else {
            budgetRepository.findByUserIdAndCategoryIdAndEffectiveFrom(
                            request.userId(), category.getId(), effectiveFrom)
                    .ifPresent(existing -> {
                        throw new ResourceConflictException(
                                "A budget limit is already defined for category " + category.getName() +
                                        " on " + effectiveFrom + "."
                        );
                    });
        }

        Budget budget = new Budget();
        budget.setUser(user);
        budget.setScope(request.scope());
        budget.setCategory(category);
        budget.setLimitAmount(request.limitAmount());
        budget.setEffectiveFrom(effectiveFrom);

        Budget savedBudget = budgetRepository.save(budget);
        return mapToResponse(savedBudget);
    }

    @Transactional
    public BudgetResponse updateBudget(Long id, UpdateBudgetRequest request) {
        Budget budget = budgetRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Budget not found with ID:" + id));

        budget.setLimitAmount(request.limitAmount());
        Budget updatedBudget = budgetRepository.save(budget);
        return mapToResponse(updatedBudget);
    }

    @Transactional
    public void deleteBudget(Long id) {
        if (!budgetRepository.existsById(id)) {
            throw new IllegalArgumentException("Budget not found with ID: " + id);
        }
        budgetRepository.deleteById(id);
    }

    private Category resolveCategory(CreateBudgetRequest request) {
        if (request.scope() == BudgetScope.GLOBAL) {
            if (request.categoryId() != null) {
                throw new IllegalArgumentException("Global budgets cannot be assigned to a category.");
            }
            return null;
        }

        if (request.categoryId() == null) {
            throw new IllegalArgumentException("Category ID is required for CATEGORY scope budgets.");
        }

        Category category = categoryRepository.findById(request.categoryId())
                .orElseThrow(() -> new IllegalArgumentException("Category not found with ID: " + request.categoryId()));

        if (category.isDeleted()) {
            throw new IllegalArgumentException("Category not found with ID: " + request.categoryId());
        }

        if (!category.getUser().getId().equals(request.userId())) {
            throw new IllegalArgumentException("Category must belong to the specified user.");
        }

        if (category.getType() != TransactionType.EXPENSE) {
            throw new IllegalArgumentException("Budgets can only be created for EXPENSE categories.");
        }

        return category;
    }

    private BudgetResponse mapToResponse(Budget budget) {
        Long categoryId = null;
        String categoryName = null;
        if (budget.getCategory() != null) {
            categoryId = budget.getCategory().getId();
            categoryName = budget.getCategory().getName();
        }

        return new BudgetResponse(
                budget.getId(),
                budget.getUser().getId(),
                budget.getScope(),
                categoryId,
                categoryName,
                budget.getLimitAmount(),
                budget.getEffectiveFrom(),
                budget.getCreatedAt(),
                budget.getUpdatedAt()
        );
    }

}
