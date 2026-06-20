import React, { createContext, useContext, useState, useCallback, ReactNode, useEffect } from 'react';
import { CheckCircle2, AlertCircle, Info, XCircle, X } from 'lucide-react';

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
    setToasts(prev => prev.map(t => t.id === id ? { ...t, exiting: true } : t) as any);
    setTimeout(() => {
      setToasts(prev => prev.filter(t => t.id !== id));
    }, 300); // Matches the exit animation duration
  }, []);

  return (
    <ToastContext.Provider value={{ addToast, removeToast }}>
      {children}
      <ToastContainer toasts={toasts as any} removeToast={removeToast} />
    </ToastContext.Provider>
  );
};

export const useToast = () => {
  const context = useContext(ToastContext);
  if (!context) throw new Error('useToast must be used within ToastProvider');
  return context;
};

const ToastItem: React.FC<{ toast: ToastMessage & { exiting?: boolean }; removeToast: (id: string) => void }> = ({ toast, removeToast }) => {
  const [isHovered, setIsHovered] = useState(false);
  
  useEffect(() => {
    let timeoutDuration = toast.duration ?? 5000;
    if (timeoutDuration < 5000) timeoutDuration = 5000;
    
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

  const variantStyles = {
    success: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 shadow-emerald-500/10',
    info: 'border-blue-500/30 bg-blue-500/10 text-blue-600 dark:text-blue-400 shadow-blue-500/10',
    warning: 'border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400 shadow-amber-500/10',
    error: 'border-red-500/30 bg-red-500/10 text-red-600 dark:text-red-400 shadow-red-500/10',
  };

  const icons = {
    success: <CheckCircle2 className="w-5 h-5 shrink-0" strokeWidth={2.5} />,
    error: <XCircle className="w-5 h-5 shrink-0" strokeWidth={2.5} />,
    warning: <AlertCircle className="w-5 h-5 shrink-0" strokeWidth={2.5} />,
    info: <Info className="w-5 h-5 shrink-0" strokeWidth={2.5} />,
  };

  const variant = toast.variant || 'info';
  const style = variantStyles[variant];
  const Icon = icons[variant];

  return (
    <div
      role={role}
      className={`app-toast-item pointer-events-auto flex items-start gap-3 rounded-[16px] border px-4 py-3 shadow-[0_8px_30px_rgb(0,0,0,0.12),0_0_20px_var(--tw-shadow-color)] backdrop-blur-xl transition-all duration-300 text-[14px] leading-relaxed ${style} ${toast.exiting ? 'opacity-0 scale-95 translate-y-4' : 'opacity-100 scale-100 translate-y-0'}`}
      style={{
        transformOrigin: 'bottom center',
        animation: toast.exiting ? 'none' : 'toast-slide-up 400ms cubic-bezier(0.16, 1, 0.3, 1) forwards'
      }}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      onFocus={() => setIsHovered(true)}
      onBlur={() => setIsHovered(false)}
    >
      <div className="mt-0.5">{Icon}</div>
      <div className="font-semibold flex-1 tracking-tight">{toast.message}</div>
      <button 
        onClick={() => removeToast(toast.id)}
        className="shrink-0 ml-2 mt-0.5 opacity-60 hover:opacity-100 hover:bg-black/5 dark:hover:bg-white/10 p-1 rounded-full transition-all active:scale-90"
        aria-label="Dismiss notification"
      >
        <X className="w-4 h-4" strokeWidth={2.5} />
      </button>
    </div>
  );
};

const ToastContainer: React.FC<{ toasts: ToastMessage[]; removeToast: (id: string) => void }> = ({ toasts, removeToast }) => {
  return (
    <div className="fixed bottom-8 left-1/2 -translate-x-1/2 z-[100] flex w-full max-w-[420px] flex-col gap-3 pointer-events-none items-center px-4">
      <style>{`
        @keyframes toast-slide-up {
          from { opacity: 0; transform: translateY(24px) scale(0.95); }
          to { opacity: 1; transform: translateY(0) scale(1); }
        }
      `}</style>
      {toasts.map(toast => (
        <ToastItem key={toast.id} toast={toast} removeToast={removeToast} />
      ))}
    </div>
  );
};
