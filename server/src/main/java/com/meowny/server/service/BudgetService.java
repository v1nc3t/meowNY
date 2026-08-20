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
import com.meowny.server.exception.ResourceNotFoundException;
import com.meowny.server.repository.BudgetRepository;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.security.CurrentUserService;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.time.LocalDate;
import java.util.List;
import java.util.stream.Collectors;

@Service
public class BudgetService {

    private final BudgetRepository budgetRepository;
    private final CategoryRepository categoryRepository;
    private final CurrentUserService currentUserService;

    public BudgetService(
            BudgetRepository budgetRepository,
            CategoryRepository categoryRepository,
            CurrentUserService currentUserService) {
        this.budgetRepository = budgetRepository;
        this.categoryRepository = categoryRepository;
        this.currentUserService = currentUserService;
    }

    @Transactional(readOnly = true)
    public BudgetResponse getBudgetById(Long id) {
        return mapToResponse(findOwnedBudget(id));
    }

    @Transactional(readOnly = true)
    public List<BudgetResponse> getCurrentUserBudgetsByPeriod(Integer year, Integer month) {
        User currentUser = currentUserService.getCurrentUser();
        LocalDate effectiveFrom = LocalDate.of(year, month, 1);
        return budgetRepository.findByUserIdAndEffectiveFrom(currentUser.getId(), effectiveFrom)
                .stream()
                .map(this::mapToResponse)
                .collect(Collectors.toList());
    }

    @Transactional
    public BudgetResponse createBudget(CreateBudgetRequest request) {
        User user = currentUserService.getCurrentUser();
        Long userId = user.getId();
        LocalDate effectiveFrom = request.effectiveFrom().withDayOfMonth(1);
        Category category = resolveCategory(request, userId);

        if (request.scope() == BudgetScope.GLOBAL) {
            budgetRepository.findGlobalByUserIdAndEffectiveFrom(userId, effectiveFrom)
                    .ifPresent(existing -> {
                        throw new ResourceConflictException(
                                "A global budget limit is already defined for " + effectiveFrom + "."
                        );
                    });
        } else {
            budgetRepository.findByUserIdAndCategoryIdAndEffectiveFrom(userId, category.getId(), effectiveFrom)
                    .ifPresent(existing -> {
                        throw new ResourceConflictException(
                                "A budget limit is already defined for category " + category.getName()
                                        + " on " + effectiveFrom + "."
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
        Budget budget = findOwnedBudget(id);
        budget.setLimitAmount(request.limitAmount());
        Budget updatedBudget = budgetRepository.save(budget);
        return mapToResponse(updatedBudget);
    }

    @Transactional
    public void deleteBudget(Long id) {
        Budget budget = findOwnedBudget(id);
        budgetRepository.delete(budget);
    }

    private Budget findOwnedBudget(Long id) {
        Budget budget = budgetRepository.findById(id)
                .orElseThrow(ResourceNotFoundException::new);
        currentUserService.requireOwnedByCurrentUser(budget.getUser().getId());
        return budget;
    }

    private Category resolveCategory(CreateBudgetRequest request, Long userId) {
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
                .orElseThrow(ResourceNotFoundException::new);

        if (category.isDeleted()) {
            throw new ResourceNotFoundException();
        }

        if (!category.getUser().getId().equals(userId)) {
            throw new ResourceNotFoundException();
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
