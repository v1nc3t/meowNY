package com.meowny.server.service;

import com.meowny.server.dto.transaction.CreateTransactionRequest;
import com.meowny.server.dto.transaction.TransactionResponse;
import com.meowny.server.dto.transaction.UpdateTransactionRequest;
import com.meowny.server.entity.Category;
import com.meowny.server.entity.RecurringTransaction;
import com.meowny.server.entity.Transaction;
import com.meowny.server.entity.User;
import com.meowny.server.exception.ResourceNotFoundException;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.repository.RecurringTransactionRepository;
import com.meowny.server.repository.TransactionRepository;
import com.meowny.server.security.CurrentUserService;
import org.springframework.data.domain.Page;
import org.springframework.data.domain.PageRequest;
import org.springframework.data.domain.Pageable;
import org.springframework.data.domain.Sort;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.util.Set;

@Service
public class TransactionService {

    private static final Set<String> ALLOWED_SORT_PROPERTIES =
            Set.of("paymentDate", "amount", "name", "createdAt", "updatedAt");

    private final TransactionRepository transactionRepository;
    private final CategoryRepository categoryRepository;
    private final RecurringTransactionRepository recurringTransactionRepository;
    private final CurrentUserService currentUserService;

    public TransactionService(
            TransactionRepository transactionRepository,
            CategoryRepository categoryRepository,
            RecurringTransactionRepository recurringTransactionRepository,
            CurrentUserService currentUserService) {
        this.transactionRepository = transactionRepository;
        this.categoryRepository = categoryRepository;
        this.recurringTransactionRepository = recurringTransactionRepository;
        this.currentUserService = currentUserService;
    }

    @Transactional(readOnly = true)
    public TransactionResponse getTransactionById(Long id) {
        Transaction tx = findOwnedTransaction(id);
        return mapToResponse(tx);
    }

    @Transactional(readOnly = true)
    public Page<TransactionResponse> getCurrentUserTransactions(Pageable pageable) {
        User currentUser = currentUserService.getCurrentUser();
        return transactionRepository.findByUserId(currentUser.getId(), sanitizePageable(pageable))
                .map(this::mapToResponse);
    }

    @Transactional
    public TransactionResponse createTransaction(CreateTransactionRequest request) {
        User user = currentUserService.getCurrentUser();
        Long userId = user.getId();

        Category category = resolveActiveCategory(request.categoryId(), userId, null);

        Transaction tx = new Transaction();
        tx.setUser(user);
        tx.setCategory(category);
        tx.setName(request.name());
        tx.setAmount(request.amount());
        tx.setPaymentDate(request.paymentDate());
        tx.setDescription(request.description());

        if (request.recurringTransactionId() != null) {
            RecurringTransaction template = recurringTransactionRepository.findById(request.recurringTransactionId())
                    .orElseThrow(ResourceNotFoundException::new);

            if (!template.getUser().getId().equals(userId)) {
                throw new ResourceNotFoundException();
            }
            if (!template.getCategory().getId().equals(request.categoryId())) {
                throw new IllegalArgumentException("Transaction category must match the recurring template category.");
            }
            tx.setSourceTemplate(template);
        }

        Transaction savedTx = transactionRepository.save(tx);
        return mapToResponse(savedTx);
    }

    @Transactional
    public TransactionResponse updateTransaction(Long id, UpdateTransactionRequest request) {
        Transaction tx = findOwnedTransaction(id);
        Long userId = tx.getUser().getId();

        Category category = resolveActiveCategory(request.categoryId(), userId, tx.getCategory().getId());

        tx.setCategory(category);
        tx.setName(request.name());
        tx.setAmount(request.amount());
        tx.setPaymentDate(request.paymentDate());
        tx.setDescription(request.description());

        Transaction updatedTx = transactionRepository.save(tx);
        return mapToResponse(updatedTx);
    }

    @Transactional
    public void deleteTransaction(Long id) {
        Transaction tx = findOwnedTransaction(id);

        if (tx.getSourceTemplate() != null) {
            tx.setSourceTemplate(null);
        }

        transactionRepository.delete(tx);
    }

    private Transaction findOwnedTransaction(Long id) {
        Transaction tx = transactionRepository.findById(id)
                .orElseThrow(ResourceNotFoundException::new);
        currentUserService.requireOwnedByCurrentUser(tx.getUser().getId());
        return tx;
    }

    private Category resolveActiveCategory(Long categoryId, Long userId, Long currentCategoryId) {
        Category category = categoryRepository.findById(categoryId)
                .orElseThrow(ResourceNotFoundException::new);

        if (category.isDeleted() && !categoryId.equals(currentCategoryId)) {
            throw new ResourceNotFoundException();
        }

        if (!category.getUser().getId().equals(userId)) {
            throw new ResourceNotFoundException();
        }

        return category;
    }

    private Pageable sanitizePageable(Pageable pageable) {
        Sort sanitized = Sort.unsorted();
        for (Sort.Order order : pageable.getSort()) {
            if (ALLOWED_SORT_PROPERTIES.contains(order.getProperty())) {
                sanitized = sanitized.and(Sort.by(order));
            }
        }
        if (sanitized.isUnsorted()) {
            sanitized = Sort.by(Sort.Direction.DESC, "paymentDate");
        }
        return PageRequest.of(pageable.getPageNumber(), pageable.getPageSize(), sanitized);
    }

    private TransactionResponse mapToResponse(Transaction tx) {
        Long templateId = null;
        String templateName = null;

        if (tx.getSourceTemplate() != null) {
            templateId = tx.getSourceTemplate().getId();
            templateName = tx.getSourceTemplate().getName();
        }

        return new TransactionResponse(
                tx.getId(),
                tx.getUser().getId(),
                tx.getCategory().getId(),
                tx.getCategory().getName(),
                templateId,
                templateName,
                tx.getCategory().getType(),
                tx.getName(),
                tx.getAmount(),
                tx.getPaymentDate(),
                tx.getDescription(),
                tx.getCreatedAt(),
                tx.getUpdatedAt()
        );
    }
}
