;;; symi-lsp.el --- Emacs configuration for Symi Language Server Protocol integration

;; Copyright (C) 2024 Symi Project

;; Author: Symi Team <contact@symi.link>
;; Version: 1.0.0
;; Package-Requires: ((lsp-mode "8.0.0") (lsp-ui "8.0.0"))
;; Keywords: languages, symi, music, microtones
;; URL: https://github.com/uiu007/symi-emacs

;;; Commentary:

;; This configuration provides complete LSP support for Symi, a notation language for microtones.
;; It includes syntax highlighting, error detection, intelligent completion, hover information,
;; and go-to-definition functionality for Emacs users.

;;; Code:

(require 'lsp-mode)
(require 'lsp-ui)

;; Define Symi language server
(lsp-register-client
 (make-lsp-client :new-connection (lsp-stdio-connection
                                  '("cargo" "run" "--bin" "lsp-server" "--" "3000"))
                 :major-modes '(symi-mode)
                 :server-id 'symi-lsp
                 :multi-root t
                 :priority 1))

;; Define Symi mode if not already defined
(unless (fboundp 'symi-mode)
  (define-derived-mode symi-mode fundamental-mode "Symi"
    "Major mode for editing Symi files."
    (setq-local comment-start "// ")
    (setq-local comment-end "")
    (setq-local font-lock-defaults '(symi-font-lock-keywords))
    (setq-local lsp-enabled t)))

;; Symi syntax highlighting
(defconst symi-font-lock-keywords
  (list
   ;; Comments
   '("//.*$" . font-lock-comment-face)
   
   ;; Keywords
   (cons (regexp-opt '("C" "D" "E" "F" "G" "A" "B" "C#" "Db" "D#" "Eb" "F#" "Gb" "G#" "Ab" "A#" "Bb") 'words)
         'font-lock-keyword-face)
   
   ;; Numbers and ratios
   '("\\b[0-9]+\\b" . font-lock-constant-face)
   '("\\b[0-9]+/[0-9]+\\b" . font-lock-constant-face)
   
   ;; Operators
   '("[@+=:;,]" . font-lock-operator-face)
   
   ;; Parentheses and brackets
   '("[()\\[\\]<>]" . font-lock-punctuation-face)
   
   ;; Macro definitions
   '("^\\([a-zA-Z_][a-zA-Z0-9_]*\\)\\s-*=" . (1 font-lock-function-name-face))
   
   ;; Base pitch definitions
   '("<\\([A-G][#b]?[0-9]\\)>" . (1 font-lock-type-face))
   
   ;; BPM definitions
   '("(\\([0-9]+\\))" . (1 font-lock-constant-face))
   
   ;; Time signatures
   '("(\\([0-9]+/[0-9]+\\))" . (1 font-lock-constant-face))
   )
  "Highlighting expressions for Symi mode.")

;; Auto-mode association
(add-to-list 'auto-mode-alist '("\\.symi\\'" . symi-mode))

;; LSP UI configuration
(setq lsp-ui-sideline-enable t)
(setq lsp-ui-sideline-show-hover t)
(setq lsp-ui-sideline-show-code-actions t)
(setq lsp-ui-doc-enable t)
(setq lsp-ui-doc-show-on-cursor t)
(setq lsp-ui-doc-position 'at-point)
(setq lsp-ui-doc-delay 0.5)

;; LSP completion settings
(setq lsp-completion-provider :capf)
(setq lsp-completion-enable t)
(setq lsp-completion-show-detail t)
(setq lsp-completion-show-kind t)

;; LSP diagnostics settings
(setq lsp-diagnostics-provider :flymake)
(setq lsp-diagnostics-echo-mode :errors)
(setq lsp-diagnostics-highlight-line t)

;; LSP server settings
(setq lsp-symi-server-command '("cargo" "run" "--bin" "lsp-server" "--" "3000"))
(setq lsp-symi-server-port 3000)

;; Key bindings for Symi mode
(defun symi-mode-setup-keybindings ()
  "Set up key bindings for Symi mode."
  (local-set-key (kbd "C-c C-c") 'lsp-execute-command)
  (local-set-key (kbd "C-c C-d") 'lsp-describe-thing-at-point)
  (local-set-key (kbd "C-c C-g") 'lsp-goto-definition)
  (local-set-key (kbd "C-c C-r") 'lsp-rename)
  (local-set-key (kbd "C-c C-f") 'lsp-format-buffer)
  (local-set-key (kbd "C-c C-i") 'lsp-organize-imports)
  (local-set-key (kbd "C-c C-e") 'lsp-execute-code-action)
  (local-set-key (kbd "C-c C-l") 'lsp-describe-session))

(add-hook 'symi-mode-hook 'symi-mode-setup-keybindings)

;; LSP workspace settings
(setq lsp-symi-workspace-settings
      '(:symi
        (:diagnostics
         (:enable t
                  :level "warning")
         :completion
         (:enable t
                  :triggerCharacters ["@" ":" ","])
         :hover
         (:enable t
                  :delay 0.3)
         :semanticTokens
         (:enable t))))

;; Custom functions for Symi development
(defun symi-start-lsp-server ()
  "Start the Symi LSP server."
  (interactive)
  (let ((port lsp-symi-server-port))
    (message "Starting Symi LSP server on port %d..." port)
    (start-process "symi-lsp-server"
                   "*symi-lsp-server*"
                   "cargo" "run" "--bin" "lsp-server" "--" (number-to-string port))
    (message "Symi LSP server started on port %d" port)))

(defun symi-stop-lsp-server ()
  "Stop the Symi LSP server."
  (interactive)
  (let ((process (get-process "symi-lsp-server")))
    (if process
        (progn
          (delete-process process)
          (message "Symi LSP server stopped"))
      (message "No Symi LSP server process found"))))

(defun symi-restart-lsp-server ()
  "Restart the Symi LSP server."
  (interactive)
  (symi-stop-lsp-server)
  (sleep-for 1)
  (symi-start-lsp-server))

;; LSP mode hooks
(add-hook 'lsp-mode-hook 'lsp-ui-mode)
(add-hook 'lsp-ui-mode-hook
          (lambda ()
            (setq-local lsp-ui-doc-enable t)
            (setq-local lsp-ui-sideline-enable t)))

;; Modeline integration
(defun symi-lsp-modeline ()
  "Display LSP status in modeline for Symi files."
  (when (and (bound-and-true-p lsp-mode)
             (eq major-mode 'symi-mode))
    (format " LSP[%s]" (lsp--workspace-print root))))
(add-to-list 'mode-line-misc-info '(:eval (symi-lsp-modeline)))

;; Error handling and logging
(setq lsp-log-io t)
(setq lsp-print-performance t)

;; Provide the package
(provide 'symi-lsp)

;;; symi-lsp.el ends here