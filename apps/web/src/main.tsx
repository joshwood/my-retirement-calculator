import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.js";
import "../../../public/assets/v1/app.css";
const root = document.querySelector("#root");
if (root === null) throw new Error("Missing application root");
createRoot(root).render(<StrictMode><App /></StrictMode>);
