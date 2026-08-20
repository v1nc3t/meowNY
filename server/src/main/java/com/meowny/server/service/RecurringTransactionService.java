package com.meowny.server.service;

import com.meowny.server.dto.recurringtransaction.CreateRecurringTransactionRequest;
import com.meowny.server.dto.recurringtransaction.RecurringTransactionResponse;
import com.meowny.server.dto.recurringtransaction.UpdateRecurringTransactionRequest;
import com.meowny.server.entity.Category;
import com.meowny.server.entity.RecurringTransaction;
import com.meowny.server.entity.User;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.exception.ResourceNotFoundException;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.repository.RecurringTransactionRepository;
import com.meowny.server.repository.TransactionRepository;
import com.meowny.server.security.CurrentUserService;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.util.List;
import java.util.stream.Collectors;

@Service
public class RecurringTransactionService {

    private final RecurringTransactionRepository recurringTransactionRepository;
    private final CategoryRepository categoryRepository;
    private final TransactionRepository transactionRepository;
    private final CurrentUserService currentUserService;

    public RecurringTransactionService(
            RecurringTransactionRepository recurringTransactionRepository,
            CategoryRepository categoryRepository,
            TransactionRepository transactionRepository,
            CurrentUserService currentUserService) {
        this.recurringTransactionRepository = recurringTransactionRepository;
        this.categoryRepository = categoryRepository;
        this.transactionRepository = transactionRepository;
        this.currentUserService = currentUserService;
    }

    @Transactional(readOnly = true)
    public List<RecurringTransactionResponse> getCurrentUserTemplates() {
        User currentUser = currentUserService.getCurrentUser();
        return recurringTransactionRepository.findByUserId(currentUser.getId())
                .stream()
                .map(this::mapToResponse)
                .collect(Collectors.toList());
    }

    @Transactional(readOnly = true)
    public RecurringTransactionResponse getTemplateById(Long id) {
        return mapToResponse(findOwnedTemplate(id));
    }

    @Transactional
    public RecurringTransactionResponse createTemplate(CreateRecurringTransactionRequest request) {
        User user = currentUserService.getCurrentUser();
        Category category = resolveActiveCategory(request.categoryId(), user.getId());

        RecurringTransaction template = new RecurringTransaction();
        template.setUser(user);
        template.setCategory(category);
        template.setName(request.name());
        template.setAmount(request.amount());
        template.setNextDueDate(request.nextDueDate());
        template.setFrequency(request.frequency());
        template.setActive(true);

        RecurringTransaction savedTemplate = recurringTransactionRepository.save(template);
        return mapToResponse(savedTemplate);
    }

    @Transactional
    public RecurringTransactionResponse updateTemplate(Long id, UpdateRecurringTransactionRequest request) {
        RecurringTransaction template = findOwnedTemplate(id);
        Category category = resolveActiveCategory(request.categoryId(), template.getUser().getId());

        template.setCategory(category);
        template.setName(request.name());
        template.setAmount(request.amount());
        template.setNextDueDate(request.nextDueDate());
        template.setFrequency(request.frequency());
        template.setActive(request.isActive());

        RecurringTransaction updatedTemplate = recurringTransactionRepository.save(template);
        return mapToResponse(updatedTemplate);
    }

    @Transactional
    public void deleteTemplate(Long id) {
        RecurringTransaction template = findOwnedTemplate(id);

        boolean isLinkedToTransactions = transactionRepository.existsBySourceTemplate_Id(id);
        if (isLinkedToTransactions) {
            throw new ResourceConflictException(
                    "Cannot delete this template because it has generated past transaction records. Disable it instead.");
        }

        recurringTransactionRepository.delete(template);
    }

    private RecurringTransaction findOwnedTemplate(Long id) {
        RecurringTransaction template = recurringTransactionRepository.findById(id)
                .orElseThrow(ResourceNotFoundException::new);
        currentUserService.requireOwnedByCurrentUser(template.getUser().getId());
        return template;
    }

    private Category resolveActiveCategory(Long categoryId, Long userId) {
        Category category = categoryRepository.findById(categoryId)
                .orElseThrow(ResourceNotFoundException::new);

        if (category.isDeleted()) {
            throw new ResourceNotFoundException();
        }

        if (!category.getUser().getId().equals(userId)) {
            throw new ResourceNotFoundException();
        }

        return category;
    }

    private RecurringTransactionResponse mapToResponse(RecurringTransaction template) {
        return new RecurringTransactionResponse(
                template.getId(),
                template.getUser().getId(),
                template.getCategory().getId(),
                template.getCategory().getName(),
                template.getCategory().getType(),
                template.getName(),
                template.getAmount(),
                template.getNextDueDate(),
                template.getFrequency(),
                template.isActive(),
                template.getCreatedAt(),
                template.getUpdatedAt()
        );
    }
}
