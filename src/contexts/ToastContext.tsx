import React, { createContext, useContext, useState, useCallback, ReactNode, useEffect } from 'react';

export type ToastVariant = 'success' | 'info' | 'warning' | 'error';

export interface ToastMessage {
  id: string;
  message: React.ReactNode;
  variant?: ToastVariant;
  duration?: number;
  isActionable?: boolean;
}

interface ToastContextType {
  addToast: (toast: Omit<ToastMessage, 'id'>) => void;
  removeToast: (id: string) => void;
}

const ToastContext = createContext<ToastContextType | undefined>(undefined);

export const ToastProvider: React.FC<{ children: ReactNode }> = ({ children }) => {
  const [toasts, setToasts] = useState<ToastMessage[]>([]);

  const addToast = useCallback((toast: Omit<ToastMessage, 'id'>) => {
    setToasts(prev => [...prev, { ...toast, id: Math.random().toString(36).substring(2, 9) }]);
  }, []);

  const removeToast = useCallback((id: string) => {
    setToasts(prev => prev.filter(t => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ addToast, removeToast }}>
      {children}
      <ToastContainer toasts={toasts} removeToast={removeToast} />
    </ToastContext.Provider>
  );
};

export const useToast = () => {
  const context = useContext(ToastContext);
  if (!context) throw new Error('useToast must be used within ToastProvider');
  return context;
};

const ToastItem: React.FC<{ toast: ToastMessage; removeToast: (id: string) => void }> = ({ toast, removeToast }) => {
  const [isHovered, setIsHovered] = useState(false);
  
  useEffect(() => {
    // Determine duration
    let timeoutDuration = toast.duration ?? 5000;
    if (timeoutDuration < 5000) timeoutDuration = 5000; // at least 5 seconds
    
    // Don't auto-dismiss actionable or important errors (unless they specified a duration)
    // Actually, "Do not auto-dismiss actionable or important errors"
    if (toast.isActionable || (toast.variant === 'error' && !toast.duration)) {
      return;
    }

    if (isHovered) {
      return;
    }

    const timer = setTimeout(() => {
      removeToast(toast.id);
    }, timeoutDuration);

    return () => clearTimeout(timer);
  }, [toast, isHovered, removeToast]);

  const role = toast.variant === 'error' ? 'alert' : 'status';

  // Variant styling
  const variantStyles = {
    success: 'border-[#10b981]/20 bg-[#064e3b]/95 text-[#34d399]',
    info: 'border-[hsl(var(--border-modal))] bg-[hsl(var(--surface-overlay))] text-[hsl(var(--text-primary))]',
    warning: 'border-[#eab308]/20 bg-[#713f12]/95 text-[#fde047]',
    error: 'border-[#ef4444]/20 bg-[#7f1d1d]/95 text-[#f87171]',
  };

  const style = variantStyles[toast.variant || 'info'];

  return (
    <div
      role={role}
      className={`app-toast-item pointer-events-auto flex items-center justify-between gap-4 rounded-xl border p-4 shadow-2xl backdrop-blur-[20px] transition-all duration-300 text-[13px] ${style}`}
      style={{ animation: 'fade-in 200ms ease-out forwards' }}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      onFocus={() => setIsHovered(true)}
      onBlur={() => setIsHovered(false)}
    >
      <div className="font-medium">{toast.message}</div>
      <button 
        onClick={() => removeToast(toast.id)}
        className="shrink-0 opacity-70 hover:opacity-100 transition-opacity"
        aria-label="Dismiss notification"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <line x1="18" y1="6" x2="6" y2="18"></line>
          <line x1="6" y1="6" x2="18" y2="18"></line>
        </svg>
      </button>
    </div>
  );
};

const ToastContainer: React.FC<{ toasts: ToastMessage[]; removeToast: (id: string) => void }> = ({ toasts, removeToast }) => {
  return (
    <div className="fixed bottom-10 left-1/2 -translate-x-1/2 z-[100] flex w-full max-w-[420px] flex-col gap-3 pointer-events-none items-center">
      {toasts.map(toast => (
        <ToastItem key={toast.id} toast={toast} removeToast={removeToast} />
      ))}
    </div>
  );
};
